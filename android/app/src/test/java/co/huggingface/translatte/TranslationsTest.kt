package co.huggingface.translatte

import android.graphics.Bitmap
import androidx.test.ext.junit.runners.AndroidJUnit4
import co.huggingface.translatte.ui.result.testRegion
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.translatte_ffi.StageTimings
import uniffi.translatte_ffi.TranslationResult
import kotlin.coroutines.EmptyCoroutineContext

@RunWith(AndroidJUnit4::class)
class TranslationsTest {
    private fun bitmap() = Bitmap.createBitmap(4, 4, Bitmap.Config.ARGB_8888)

    private fun result(text: String) = TranslationResult(
        4u,
        4u,
        listOf(testRegion(text, text, 0f, 0f)),
        null,
        StageTimings(0u, 0u, 0u),
    )

    /** Each call to the fake engine waits for the test to hand it a result, ignoring cancellation like the native call. */
    private class FakeEngine {
        val calls = mutableListOf<CompletableDeferred<Result<TranslationResult>>>()

        suspend fun translate(@Suppress("UNUSED_PARAMETER") bitmap: Bitmap, onStage: (String) -> Unit): TranslationResult {
            val call = CompletableDeferred<Result<TranslationResult>>().also { calls += it }
            onStage("Finding text…")
            return withContext(NonCancellable) { call.await() }.getOrThrow()
        }
    }

    private fun CoroutineScope.translations(engine: FakeEngine) =
        Translations(this, EmptyCoroutineContext, engine::translate) { it.message.orEmpty() }

    @Test
    fun aNewPhotoSupersedesARunningTranslation() = runBlocking {
        val engine = FakeEngine()
        val t = translations(engine)
        val first = bitmap()
        val second = bitmap()
        t.start { first }
        yield()
        t.start { second }
        yield()
        assertEquals(2, engine.calls.size)

        val secondResult = result("second")
        engine.calls[1].complete(Result.success(secondResult))
        yield()
        assertEquals(Phase.Done, t.phase)
        assertSame(second, t.image)

        engine.calls[0].complete(Result.success(result("first")))
        yield()
        yield()
        assertSame(second, t.image)
        assertSame(secondResult, t.result)
        assertEquals(Phase.Done, t.phase)
        assertSame(secondResult, t.lastSuccess?.result)
    }

    @Test
    fun aStaleFailureIsDropped() = runBlocking {
        val engine = FakeEngine()
        val t = translations(engine)
        t.start { bitmap() }
        yield()
        t.start { bitmap() }
        yield()
        engine.calls[0].complete(Result.failure(IllegalStateException("stale")))
        yield()
        yield()
        assertEquals(Phase.Working("Finding text…"), t.phase)
        engine.calls[1].complete(Result.success(result("ok")))
        yield()
        assertEquals(Phase.Done, t.phase)
    }

    @Test
    fun aPhotoThatFailsToLoadShowsTheErrorAndKeepsTheLastSuccess() = runBlocking {
        val engine = FakeEngine()
        val t = translations(engine)
        val good = bitmap()
        t.start { good }
        yield()
        engine.calls[0].complete(Result.success(result("ok")))
        yield()
        t.start { throw java.io.IOException("unreadable") }.join()

        assertEquals(Phase.Failed("Could not open the photo"), t.phase)
        assertNull(t.image)
        assertNull(t.result)
        assertSame(good, t.lastSuccess?.image)
    }

    @Test
    fun aFailedTranslationKeepsTheLastSuccess() = runBlocking {
        val engine = FakeEngine()
        val t = translations(engine)
        val good = bitmap()
        t.start { good }
        yield()
        engine.calls[0].complete(Result.success(result("ok")))
        yield()
        val job = t.start { bitmap() }
        yield()
        engine.calls[1].complete(Result.failure(IllegalStateException("engine broke")))
        job.join()

        assertEquals(Phase.Failed("engine broke"), t.phase)
        assertSame(good, t.lastSuccess?.image)

        t.reopenLast()
        assertEquals(Phase.Done, t.phase)
        assertSame(good, t.image)
    }

    @Test
    fun noTextIsNotASuccess() = runBlocking {
        val engine = FakeEngine()
        val t = translations(engine)
        val job = t.start { bitmap() }
        yield()
        engine.calls[0].complete(Result.success(TranslationResult(4u, 4u, emptyList(), null, StageTimings(0u, 0u, 0u))))
        job.join()
        assertEquals(Phase.Failed("No text found. Try moving closer or reducing glare."), t.phase)
        assertNull(t.lastSuccess)
    }

    @Test
    fun aLiveResultSupersedesARunningTranslationAndBecomesTheLastSuccess() = runBlocking {
        val engine = FakeEngine()
        val t = translations(engine)
        t.start { bitmap() }
        yield()
        val frame = bitmap()
        val live = result("live")
        t.show(frame, live)
        assertEquals(Phase.Done, t.phase)
        assertSame(frame, t.image)
        assertSame(live, t.result)
        assertSame(live, t.lastSuccess?.result)

        engine.calls[0].complete(Result.success(result("stale")))
        yield()
        yield()
        assertSame(live, t.result)
    }
}
