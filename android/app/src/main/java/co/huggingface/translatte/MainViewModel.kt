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
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import uniffi.translatte_ffi.FfiException
import uniffi.translatte_ffi.LanguageInfo
import uniffi.translatte_ffi.ModelInfo
import uniffi.translatte_ffi.ProgressListener
import uniffi.translatte_ffi.TranslationResult
import java.io.File

private const val TAG = "translatte"

/**
 * Memory the loaded models need besides the memory-mapped weights (ONNX Runtime, llama.cpp's KV
 * cache and compute buffers): peak anonymous RSS was ~840MB translating a photo, plus margin.
 * Below this, loading risks the low-memory killer, so the app refuses.
 */
private const val MIN_FREE_MEMORY = 1_200_000_000L

/**
 * Also keeping the weights resident (~2.6GB of file-backed RSS at peak), plus a margin. Well below
 * this the kernel evicts weight pages and re-reads them from flash during decode, which made
 * translation ~2x slower on a Pixel 8 (which typically has 3.0-3.6GB available).
 */
private const val COMFORTABLE_FREE_MEMORY = 2_800_000_000L

private class LowMemoryException(message: String) : Exception(message)

/**
 * The saved [source] and [target] if both are still offered, else replacements: a choice can
 * disappear when an app update drops a language or its script pack is deleted. A replacement is
 * English if possible, else the first language in the (alphabetical) list, and never the same as
 * the other side; the target changes if only English is left to read. Same rule as iOS's
 * `LanguageDirection.fallback`.
 */
fun fallbackLanguages(
    sources: List<LanguageInfo>,
    targets: List<LanguageInfo>,
    source: String,
    target: String,
): Pair<String, String> {
    fun pick(languages: List<LanguageInfo>, avoid: String?) =
        (languages.find { it.code == "en" && it.code != avoid } ?: languages.find { it.code != avoid })?.code
    var s = if (sources.any { it.code == source }) source else pick(sources, target) ?: pick(sources, null) ?: ""
    var t = if (targets.any { it.code == target }) target else pick(targets, s) ?: ""
    if (s == t && (s != source || t != target)) t = pick(targets, s) ?: t
    return s to t
}

/** Swapping makes the target the new source, so it must be readable with the installed packs. */
fun canSwap(sourceLanguages: List<LanguageInfo>, target: String): Boolean = sourceLanguages.any { it.code == target }

class MainViewModel(app: Application) : AndroidViewModel(app) {
    private val translatte = app as TranslatteApp
    private val engine get() = translatte.engine
    private val prefs = app.getSharedPreferences("settings", Context.MODE_PRIVATE)

    var models by mutableStateOf<List<ModelInfo>>(emptyList())
        private set
    var sourceLanguages by mutableStateOf<List<LanguageInfo>>(emptyList())
        private set
    var targetLanguages by mutableStateOf<List<LanguageInfo>>(emptyList())
        private set
    var ready by mutableStateOf(false)
        private set
    var setupError by mutableStateOf<String?>(null)
        private set
    var sourceCode by mutableStateOf(prefs.getString("source", "") ?: "")
        private set
    var targetCode by mutableStateOf(prefs.getString("target", "en") ?: "en")
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
            sourceLanguages = engine.sourceLanguages()
            targetLanguages = engine.targetLanguages()
            val wasReady = ready
            ready = engine.coreInstalled() && sourceLanguages.isNotEmpty()
            val (source, target) = fallbackLanguages(sourceLanguages, targetLanguages, sourceCode, targetCode)
            if (target != targetCode) {
                targetCode = target
                prefs.edit().putString("target", target).apply()
            }
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

    fun setTarget(code: String) {
        targetCode = code
        prefs.edit().putString("target", code).apply()
        prepare()
    }

    fun swap() {
        if (!canSwap(sourceLanguages, targetCode)) return
        val (source, target) = sourceCode to targetCode
        sourceCode = target
        targetCode = source
        prefs.edit().putString("source", target).putString("target", source).apply()
        prepare()
    }

    /**
     * Loads the models and prefills the translator's prompt prefix for the picked languages, so a
     * photo taken next skips that work. Called when the languages change and when the app returns
     * to the foreground.
     */
    fun prepare() {
        if (!ready || sourceCode.isEmpty()) return
        val (source, target) = sourceCode to targetCode
        viewModelScope.launch(translatte.engineDispatcher) {
            try {
                checkMemory()
                engine.prepare(source, target)
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
     * mid-translation. Warns below [COMFORTABLE_FREE_MEMORY] and clears the warning once memory
     * recovers. Runs on the engine dispatcher.
     */
    private suspend fun checkMemory() {
        val app = getApplication<Application>()
        val info = ActivityManager.MemoryInfo().also(app.getSystemService(ActivityManager::class.java)::getMemoryInfo)
        val free = formatSize(info.availMem)
        if (info.availMem < MIN_FREE_MEMORY && !translatte.modelsLoaded) {
            throw LowMemoryException(
                "Not enough free memory to load the translator ($free free, needs " +
                    "${formatSize(MIN_FREE_MEMORY)}). Close other apps and try again.",
            )
        }
        val warning = if (info.availMem < COMFORTABLE_FREE_MEMORY) {
            "Low memory ($free free): translation will be slow. Close other apps to speed it up."
        } else {
            null
        }
        withContext(Dispatchers.Main) { memoryWarning = warning }
    }

    /** Bytes still to fetch for [ids], counting partial downloads that will resume. */
    fun bytesToDownload(ids: List<String>): Long =
        models.filter { it.id in ids }.sumOf { (it.sizeBytes - it.downloadedBytes).toLong() }

    fun deleteModel(id: String) {
        viewModelScope.launch {
            val error = withContext(translatte.engineDispatcher) {
                runCatching {
                    // Deleting the pack in use frees every loaded model, the translator included.
                    if (engine.deleteModel(id)) translatte.modelsLoaded = false
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

    fun retry() = translations.retry()

    fun reopenLast() = translations.reopenLast()

    private suspend fun translateOnEngine(bitmap: Bitmap, attempt: Int, onStage: (String) -> Unit): TranslationResult {
        val (source, target) = sourceCode to targetCode
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
            engine.translateImage(Images.jpeg(bitmap), source, target, true, attempt.toUInt(), listener).also {
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

    /**
     * Debug-build hook for scripted device tests: translates [path] and writes timing and regions
     * to [out] as JSON.
     */
    fun autotest(path: File, from: String?, to: String?, out: File) = viewModelScope.launch {
        from?.let(::setSource)
        to?.let(::setTarget)
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
