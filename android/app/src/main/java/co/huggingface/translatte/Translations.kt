package co.huggingface.translatte

import android.graphics.Bitmap
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.translatte_ffi.TranslationResult
import kotlin.coroutines.CoroutineContext

sealed interface Phase {
    data object Idle : Phase
    data class Working(val label: String) : Phase
    data object Done : Phase
    /** [retryable]: running the same photo again could succeed. */
    data class Failed(val message: String, val retryable: Boolean = true) : Phase
}

data class Translated(val image: Bitmap, val result: TranslationResult)

/**
 * The photo being translated and its outcome. A new photo supersedes a running translation: the old
 * job is cancelled and, since the engine call itself can't be interrupted, a generation check drops
 * whatever it still returns. [lastSuccess] changes only when a translation succeeds.
 */
class Translations(
    private val scope: CoroutineScope,
    private val io: CoroutineContext,
    /** Translates a photo; `attempt` is 0 for a new photo and counts retries of the same one. */
    private val translate: suspend (Bitmap, attempt: Int, onStage: (String) -> Unit) -> TranslationResult,
    private val errorMessage: (Throwable) -> String,
) {
    var image by mutableStateOf<Bitmap?>(null)
        private set
    var result by mutableStateOf<TranslationResult?>(null)
        private set
    var phase by mutableStateOf<Phase>(Phase.Idle)
        private set
    var lastSuccess by mutableStateOf<Translated?>(null)
        private set

    private var job: Job? = null
    private var generation = 0
    private var attempt = 0

    /** Loads a photo with [load] (on the IO context) and translates it. */
    fun start(load: () -> Bitmap): Job {
        val gen = supersede()
        attempt = 0
        image = null
        result = null
        phase = Phase.Working("Loading photo…")
        return scope.launch {
            val bitmap = try {
                withContext(io) { load() }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                if (gen == generation) phase = Phase.Failed("Could not open the photo", retryable = false)
                return@launch
            }
            run(gen, bitmap)
        }.also { job = it }
    }

    /** Runs the current photo again, e.g. after an error, with a new [attempt] so sampling varies. */
    fun retry() {
        val bitmap = image ?: return
        val gen = supersede()
        attempt++
        job = scope.launch { run(gen, bitmap) }
    }

    /** Shows [lastSuccess] again unless a translation is running. */
    fun reopenLast() {
        if (phase is Phase.Working) return
        val last = lastSuccess ?: return
        supersede()
        image = last.image
        result = last.result
        phase = Phase.Done
    }

    private fun supersede(): Int {
        job?.cancel()
        return ++generation
    }

    private suspend fun run(gen: Int, bitmap: Bitmap) {
        if (gen != generation) return
        image = bitmap
        result = null
        phase = Phase.Working("Starting…")
        try {
            val output = translate(bitmap, attempt) { label ->
                scope.launch { if (gen == generation && phase is Phase.Working) phase = Phase.Working(label) }
            }
            if (gen != generation) return
            result = output
            phase = when {
                output.translationError != null -> Phase.Failed("Translation failed: ${output.translationError}")
                output.regions.isEmpty() -> Phase.Failed("No text found. Try moving closer or reducing glare.", retryable = false)
                else -> Phase.Done.also { lastSuccess = Translated(bitmap, output) }
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            if (gen == generation) phase = Phase.Failed(errorMessage(e))
        }
    }
}
