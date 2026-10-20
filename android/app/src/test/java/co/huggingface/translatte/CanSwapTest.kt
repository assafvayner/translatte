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

    private val targets = listOf(
        LanguageInfo("ar", "Arabic", true),
        LanguageInfo("en", "English", false),
        LanguageInfo("fr", "French", false),
    )

    @Test
    fun removedLanguagesFallBackWithoutPairingALanguageWithItself() {
        assertEquals("fr" to "en", fallbackLanguages(sources, targets, "fr", "en"))
        assertEquals("en" to "fr", fallbackLanguages(sources, targets, "sv", "fr"))
        assertEquals("fr" to "en", fallbackLanguages(sources, targets, "sv", "en"))
        assertEquals("en" to "ar", fallbackLanguages(sources, targets, "en", "sv"))
        assertEquals("en" to "ar", fallbackLanguages(sources, targets, "sv", "no"))
        assertEquals("en" to "ar", fallbackLanguages(sources.drop(1), targets, "sv", "en"))
        assertEquals("" to "en", fallbackLanguages(emptyList(), targets, "sv", "en"))
    }
}
