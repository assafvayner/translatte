package co.huggingface.translatte

import android.app.ActivityManager
import android.app.Application
import android.content.Context
import android.graphics.Bitmap
import android.util.Log
import android.net.Uri
import android.os.SystemClock
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import co.huggingface.translatte.ui.camera.LiveFrame
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import uniffi.translatte_ffi.FfiException
import uniffi.translatte_ffi.LanguageInfo
import uniffi.translatte_ffi.LanguagePack
import uniffi.translatte_ffi.ModelInfo
import uniffi.translatte_ffi.ProgressListener
import uniffi.translatte_ffi.TranslationResult
import java.io.File

private const val TAG = "translatte"

/**
 * Memory the loaded models need, with margin: OCR plus a pair model peaked near 335MB (CJK) in the
 * native benchmark on a Pixel 8. Below this, loading risks the low-memory killer, so the app refuses.
 */
private const val MIN_FREE_MEMORY = 800_000_000L

private class LowMemoryException(message: String) : Exception(message)

/**
 * The saved [source] if it's still offered, else the first language in the (alphabetical) list: a
 * choice can disappear when an app update drops a language or its pack is deleted. Same rule as iOS's
 * `LanguageDirection.fallback`.
 */
fun fallbackSource(sources: List<LanguageInfo>, source: String): String =
    if (sources.any { it.code == source }) source else sources.firstOrNull()?.code ?: ""

class MainViewModel(app: Application) : AndroidViewModel(app) {
    private val translatte = app as TranslatteApp
    private val engine get() = translatte.engine
    private val prefs = app.getSharedPreferences("settings", Context.MODE_PRIVATE)

    var models by mutableStateOf<List<ModelInfo>>(emptyList())
        private set
    var sourceLanguages by mutableStateOf<List<LanguageInfo>>(emptyList())
        private set
    var packs by mutableStateOf<List<LanguagePack>>(emptyList())
        private set
    var target by mutableStateOf<LanguageInfo?>(null)
        private set
    var ready by mutableStateOf(false)
        private set
    var setupError by mutableStateOf<String?>(null)
        private set
    var sourceCode by mutableStateOf(prefs.getString("source", "") ?: "")
        private set
    val translations = Translations(viewModelScope, Dispatchers.IO, ::translateOnEngine, ::errorMessage)
    val image get() = translations.image
    val result get() = translations.result
    val phase get() = translations.phase
    val lastSuccess get() = translations.lastSuccess

    /** Shown above the photo when free memory is short; null when there's plenty. */
    var memoryWarning by mutableStateOf<String?>(null)
        private set

    init {
        refresh()
    }

    fun refresh() {
        try {
            models = engine.models()
            packs = engine.languagePacks()
            sourceLanguages = engine.sourceLanguages()
            target = engine.targetLanguage()
            val wasReady = ready
            ready = engine.coreInstalled() && sourceLanguages.isNotEmpty()
            val source = fallbackSource(sourceLanguages, sourceCode)
            if (source != sourceCode) {
                setSource(source)
            } else if (ready && !wasReady) {
                prepare()
            }
            setupError = null
        } catch (e: Exception) {
            setupError = errorMessage(e)
            ready = false
        }
    }

    fun setSource(code: String) {
        sourceCode = code
        prefs.edit().putString("source", code).apply()
        prepare()
    }

    /**
     * Loads the models for the picked source language and runs the translation model once, so a photo
     * taken next skips that work. Called when the source changes and when the app returns to the
     * foreground.
     */
    fun prepare() {
        if (!ready || sourceCode.isEmpty()) return
        val source = sourceCode
        viewModelScope.launch(translatte.engineDispatcher) {
            try {
                checkMemory()
                engine.prepare(source)
                translatte.modelsLoaded = true
            } catch (e: LowMemoryException) {
                withContext(Dispatchers.Main) { memoryWarning = e.message }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                Log.w(TAG, "prepare failed: $e")
            }
        }
    }

    /**
     * Before preparing or translating: when the models aren't loaded yet, refuses below
     * [MIN_FREE_MEMORY] rather than letting the low-memory killer take the app (or others) down
     * mid-translation, and clears the warning once memory recovers. Runs on the engine dispatcher.
     */
    private suspend fun checkMemory() {
        if (!translatte.modelsLoaded) {
            val app = getApplication<Application>()
            val info = ActivityManager.MemoryInfo().also(app.getSystemService(ActivityManager::class.java)::getMemoryInfo)
            if (info.availMem < MIN_FREE_MEMORY) {
                throw LowMemoryException(
                    "Not enough free memory to load the models (${formatSize(info.availMem)} free, needs " +
                        "${formatSize(MIN_FREE_MEMORY)}). Close other apps and try again.",
                )
            }
        }
        if (memoryWarning != null) withContext(Dispatchers.Main) { memoryWarning = null }
    }

    /** Bytes still to fetch for [ids], counting partial downloads that will resume. */
    fun bytesToDownload(ids: List<String>): Long =
        models.filter { it.id in ids }.sumOf { (it.sizeBytes - it.downloadedBytes).toLong() }

    fun deleteLanguage(code: String) {
        viewModelScope.launch {
            val error = withContext(translatte.engineDispatcher) {
                runCatching {
                    // Deleting the script in use frees every loaded model.
                    if (engine.deleteLanguage(code)) translatte.modelsLoaded = false
                }.exceptionOrNull()
            }
            refresh()
            error?.let { setupError = errorMessage(it) }
        }
    }

    fun translate(uri: Uri) = translations.start { Images.load(getApplication<Application>().contentResolver, uri) }

    /** [file] is a one-off capture, deleted once decoded. */
    fun translate(file: File) = translations.start {
        try {
            Images.load(file)
        } finally {
            file.delete()
        }
    }

    fun reopenLast() = translations.reopenLast()

    fun showLive(image: Bitmap, result: TranslationResult) = translations.show(image, result)

    /** Reads one live camera frame (OCR, untranslated) from the current source language on the engine dispatcher. */
    suspend fun readFrame(frame: LiveFrame): TranslationResult {
        val source = sourceCode
        return withContext(translatte.engineDispatcher) {
            try {
                checkMemory()
            } catch (e: LowMemoryException) {
                if (memoryWarning != e.message) withContext(Dispatchers.Main) { memoryWarning = e.message }
                throw e
            }
            engine.ocrFrame(frame.rgba, frame.width.toUInt(), frame.height.toUInt(), frame.rotation.toUInt(), source).also {
                translatte.modelsLoaded = true
            }
        }
    }

    /** Translates the regions [readFrame] read, while the engine dispatcher reads the next frame. */
    suspend fun translateRegions(read: TranslationResult): TranslationResult {
        val source = sourceCode
        return withContext(translatte.liveTranslateDispatcher) { engine.translateRegions(read, source) }
    }

    private suspend fun translateOnEngine(bitmap: Bitmap, onStage: (String) -> Unit): TranslationResult {
        val source = sourceCode
        val listener = object : ProgressListener {
            override fun onStage(stage: String, done: UInt, total: UInt) {
                onStage(
                    when (stage) {
                        "Detecting" -> "Finding text…"
                        "Recognizing" -> "Reading text ${done + 1u}/$total…"
                        "Translating" -> if (total > 1u) "Translating ${done + 1u}/$total…" else "Translating…"
                        else -> stage
                    },
                )
            }
        }
        return withContext(translatte.engineDispatcher) {
            checkMemory()
            engine.translateImage(Images.jpeg(bitmap), source, true, listener).also {
                translatte.modelsLoaded = true
            }
        }
    }

    /** Covers uniffi's InternalException (a Rust panic) and anything else, not just FfiException. */
    private fun errorMessage(e: Throwable): String = when (e) {
        is FfiException.Failed -> e.reason
        is LowMemoryException -> e.message.orEmpty()
        else -> e.toString()
    }

    /** Debug-build hook for scripted device tests: translates [path] from [from] and writes timing and regions to [out] as JSON. */
    fun autotest(path: File, from: String?, out: File) = viewModelScope.launch {
        from?.let(::setSource)
        val start = SystemClock.elapsedRealtime()
        translations.start { Images.load(path) }.join()
        val regions = JSONArray()
        result?.regions?.forEach {
            regions.put(JSONObject().put("source", it.sourceText).put("translated", it.translatedText ?: JSONObject.NULL))
        }
        val summary = JSONObject()
            .put("phase", phase.toString())
            .put("seconds", (SystemClock.elapsedRealtime() - start) / 1000.0)
            .put("regions", regions)
        withContext(Dispatchers.IO) { out.writeText(summary.toString(2)) }
    }
}
