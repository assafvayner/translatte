package co.huggingface.translatte

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.translatte_ffi.LanguageInfo

class FallbackSourceTest {
    private val sources = listOf(LanguageInfo("fr", "French", false), LanguageInfo("ja", "Japanese", false))

    @Test
    fun keepsAnOfferedSource() {
        assertEquals("ja", fallbackSource(sources, "ja"))
    }

    @Test
    fun replacesARemovedSourceWithTheFirstOffered() {
        assertEquals("fr", fallbackSource(sources, "en"))
        assertEquals("", fallbackSource(emptyList(), "fr"))
    }
}
