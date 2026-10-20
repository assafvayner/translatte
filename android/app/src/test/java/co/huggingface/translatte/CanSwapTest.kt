package co.huggingface.translatte

import org.junit.Assert.assertEquals
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

    @Test
    fun aRemovedLanguageFallsBackToEnglish() {
        assertEquals("fr", availableOrDefault(sources, "fr"))
        assertEquals("en", availableOrDefault(sources, "sv"))
        assertEquals("fr", availableOrDefault(sources.take(1), "sv"))
        assertEquals("", availableOrDefault(emptyList(), "sv"))
    }
}
