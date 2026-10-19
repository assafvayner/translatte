package co.huggingface.translatte

import android.app.ActivityManager
import android.app.Application
import android.content.Context
import android.text.format.Formatter
import android.util.Log
import android.graphics.Bitmap
import android.net.Uri
import android.os.SystemClock
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
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

sealed interface Phase {
    data object Idle : Phase
    data class Working(val label: String) : Phase
    data object Done : Phase
    data class Failed(val message: String) : Phase
}

private const val TAG = "translatte"

/** Below this, loading the translator risks the low-memory killer; see [MainViewModel.checkMemory]. */
private const val MIN_FREE_MEMORY = 1_500_000_000L

private class LowMemoryException(message: String) : Exception(message)

class MainViewModel(app: Application) : AndroidViewModel(app) {
    private val translatte = app as TranslatteApp
    private val engine = translatte.engine
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
    var image by mutableStateOf<Bitmap?>(null)
        private set
    var result by mutableStateOf<TranslationResult?>(null)
        private set
    var phase by mutableStateOf<Phase>(Phase.Idle)
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
            if (sourceLanguages.none { it.code == sourceCode }) {
                setSource(sourceLanguages.firstOrNull()?.code ?: "")
            } else if (ready && !wasReady) {
                prepare()
            }
            setupError = null
        } catch (e: FfiException) {
            setupError = e.toString()
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
            } catch (e: Exception) {
                Log.w(TAG, "prepare failed: $e")
            }
        }
    }

    /**
     * Refuses to load the translator when the system is short on memory, rather than letting the
     * low-memory killer take the app (or others) down mid-translation. The ~2.4GB of weights are
     * memory-mapped and count as reclaimable cache, so only the rest has to be free.
     */
    private fun checkMemory() {
        if (translatte.modelsLoaded) return
        val am = getApplication<Application>().getSystemService(ActivityManager::class.java)
        val info = ActivityManager.MemoryInfo().also(am::getMemoryInfo)
        if (info.availMem < MIN_FREE_MEMORY) {
            val app = getApplication<Application>()
            throw LowMemoryException(
                "Not enough free memory to load the translator (" +
                    "${Formatter.formatShortFileSize(app, info.availMem)} free, needs " +
                    "${Formatter.formatShortFileSize(app, MIN_FREE_MEMORY)}). Close other apps and try again.",
            )
        }
    }

    fun deleteModel(id: String) {
        viewModelScope.launch {
            val error = withContext(translatte.engineDispatcher) {
                runCatching { engine.deleteModel(id) }.exceptionOrNull()
            }
            refresh()
            error?.let { setupError = (it as? FfiException.Failed)?.reason ?: it.toString() }
        }
    }

    fun translate(uri: Uri) = viewModelScope.launch {
        val bitmap = loadOrFail { Images.load(getApplication<Application>().contentResolver, uri) } ?: return@launch
        run(bitmap)
    }

    private suspend fun loadOrFail(load: () -> Bitmap): Bitmap? {
        phase = Phase.Working("Loading photo…")
        return withContext(Dispatchers.IO) { runCatching(load) }.getOrElse {
            phase = Phase.Failed("Could not open the photo: ${it.message}")
            null
        }
    }

    private suspend fun run(bitmap: Bitmap) {
        image = bitmap
        result = null
        phase = Phase.Working("Starting…")
        val (source, target) = sourceCode to targetCode
        val listener = object : ProgressListener {
            override fun onStage(stage: String, done: UInt, total: UInt) {
                val label = when (stage) {
                    "Detecting" -> "Finding text…"
                    "Recognizing" -> "Reading text ${done + 1u}/$total…"
                    "Translating" -> if (total > 1u) "Translating ${done + 1u}/$total…" else "Translating…"
                    else -> stage
                }
                viewModelScope.launch { if (phase is Phase.Working) phase = Phase.Working(label) }
            }
        }
        try {
            val output = withContext(translatte.engineDispatcher) {
                checkMemory()
                engine.translateImage(Images.jpeg(bitmap), source, target, true, listener).also {
                    translatte.modelsLoaded = true
                }
            }
            result = output
            phase = when {
                output.translationError != null -> Phase.Failed("Translation failed: ${output.translationError}")
                output.regions.isEmpty() -> Phase.Failed("No text found. Try moving closer or reducing glare.")
                else -> Phase.Done
            }
        } catch (e: FfiException) {
            phase = Phase.Failed((e as? FfiException.Failed)?.reason ?: e.toString())
        } catch (e: LowMemoryException) {
            phase = Phase.Failed(e.message.orEmpty())
        }
    }

    /**
     * Debug-build hook for scripted device tests: translates [path] and writes timing and regions
     * to [out] as JSON.
     */
    fun autotest(path: File, from: String?, to: String?, out: File) = viewModelScope.launch {
        from?.let(::setSource)
        to?.let(::setTarget)
        val start = SystemClock.elapsedRealtime()
        loadOrFail { Images.load(path) }?.let { run(it) }
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
