package co.huggingface.translatte.ui.camera

import androidx.test.ext.junit.runners.AndroidJUnit4
import co.huggingface.translatte.ui.result.testResult
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.job
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.translatte_ffi.TranslationResult

@RunWith(AndroidJUnit4::class)
class LiveTranslatorTest {
    /** A stage whose every call waits for the test to complete it. */
    private class FakeStage {
        val calls = mutableListOf<CompletableDeferred<TranslationResult>>()

        suspend fun run(): TranslationResult = CompletableDeferred<TranslationResult>().also { calls += it }.await()
    }

    private val reads = FakeStage()
    private val translations = FakeStage()

    private fun live(scope: CoroutineScope) =
        LiveTranslator(scope, { reads.run() }, { translations.run() }, { false }).apply { enabled = true }

    private fun LiveTranslator.offerFrame() = offer { LiveFrame(ByteArray(4), 1, 1, 0) }

    private suspend fun settle() = repeat(10) { yield() }

    @Test
    fun aFrameReadBeforeAResetIsNotTranslated() = runBlocking {
        val camera = CoroutineScope(coroutineContext + Job(coroutineContext.job))
        val live = live(camera)
        live.offerFrame()
        settle()
        live.reset()
        reads.calls[0].complete(testResult())
        settle()
        assertEquals(0, translations.calls.size)

        live.offerFrame()
        settle()
        reads.calls[1].complete(testResult())
        settle()
        assertEquals(1, translations.calls.size)
        camera.coroutineContext.job.cancel()
    }

    @Test
    fun aCancelledTranslationLeavesTheStagesReady() = runBlocking {
        val camera = CoroutineScope(coroutineContext + Job(coroutineContext.job))
        val live = live(camera)
        live.offerFrame()
        settle()
        reads.calls[0].complete(testResult())
        settle()
        live.offerFrame()
        settle()
        reads.calls[1].complete(testResult())
        settle()
        assertEquals("the second frame waits for the first's translation", 1, translations.calls.size)

        camera.coroutineContext.job.cancelChildren()
        settle()
        live.offerFrame()
        settle()
        reads.calls[2].complete(testResult())
        settle()
        assertEquals(2, translations.calls.size)
        camera.coroutineContext.job.cancel()
    }
}
