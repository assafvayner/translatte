package co.huggingface.translatte

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.translatte_ffi.LanguageInfo

class CanSwapTest {
    private val sources = listOf(LanguageInfo("fr", "French", false), LanguageInfo("en", "English", false))

    @Test
    fun swapNeedsTheTargetToBeReadable() {
        assertTrue(canSwap(sources, "en"))
        assertFalse(canSwap(sources, "ja"))
    }
}
