package co.huggingface.translatte.ui.setup

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.translatte_ffi.LanguagePack
import uniffi.translatte_ffi.ModelInfo
import uniffi.translatte_ffi.ModelState

class ModelsScreenLogicTest {
    private fun model(id: String, state: ModelState) = ModelInfo(id, id, false, 10u, 0u, state, emptyList())

    @Test
    fun missingModelsSkipsInstalledOnesAndDuplicates() {
        val models = listOf(
            model("det", ModelState.INSTALLED),
            model("rec-latin", ModelState.NOT_INSTALLED),
            model("pair-fr-en", ModelState.INCOMPLETE),
        )
        assertEquals(
            listOf("rec-latin", "pair-fr-en"),
            missingModels(models, listOf("det", "rec-latin", "pair-fr-en", "rec-latin")),
        )
    }

    @Test
    fun packProgressSumsItsModels() {
        val progress = mapOf("rec-latin" to (5L to 10L), "pair-fr-en" to (1L to 30L), "det" to (3L to 3L))
        assertEquals(6L to 40L, packProgress(progress, listOf("rec-latin", "pair-fr-en")))
        assertNull(packProgress(progress, listOf("pair-de-en")))
    }

    @Test
    fun rowOwnsADownloadOnlyWhenAllItsMissingModelsAreInFlight() {
        val inFlight = setOf("rec-latin", "pair-fr-en")
        assertTrue(ownsDownload(listOf("rec-latin", "pair-fr-en"), inFlight))
        assertTrue(ownsDownload(listOf("pair-fr-en"), inFlight))
        assertFalse(ownsDownload(listOf("rec-latin", "pair-de-en"), inFlight))
        assertFalse(ownsDownload(emptyList(), inFlight))
    }

    @Test
    fun packStatusShowsRemainingBytesUnlessInstalled() {
        fun pack(state: ModelState) = LanguagePack("fr", "French", listOf("rec-latin", "pair-fr-en"), 50_071_468u, 8_044_639u, state, false)
        assertEquals("50.1 MB", packStatus(pack(ModelState.NOT_INSTALLED).copy(downloadedBytes = 0u)))
        assertEquals("42.0 MB", packStatus(pack(ModelState.NOT_INSTALLED)))
        assertEquals("Incomplete · 42.0 MB", packStatus(pack(ModelState.INCOMPLETE)))
        assertEquals("Installed · 50.1 MB", packStatus(pack(ModelState.INSTALLED)))
    }
}
