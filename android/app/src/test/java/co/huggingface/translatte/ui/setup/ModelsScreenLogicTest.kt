package co.huggingface.translatte.ui.setup

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
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
}
