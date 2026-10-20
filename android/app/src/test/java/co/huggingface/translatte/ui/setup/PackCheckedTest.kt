package co.huggingface.translatte.ui.setup

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PackCheckedTest {
    @Test
    fun manageModeOnlyChecksInstalledPacks() {
        assertFalse(packChecked(core = false, installed = false, setup = false, selected = true))
        assertTrue(packChecked(core = false, installed = true, setup = false, selected = false))
    }

    @Test
    fun setupChecksSelectedPacks() {
        assertTrue(packChecked(core = false, installed = false, setup = true, selected = true))
        assertFalse(packChecked(core = false, installed = false, setup = true, selected = false))
    }

    @Test
    fun coreModelsAreAlwaysChecked() {
        assertTrue(packChecked(core = true, installed = false, setup = false, selected = false))
    }
}
