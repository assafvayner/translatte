package co.huggingface.translatte.ui.settings

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ThemeSettingTest {
    @Test
    fun systemFollowsThePhone() {
        assertFalse(ThemeSetting.SYSTEM.isDark(systemDark = false))
        assertTrue(ThemeSetting.SYSTEM.isDark(systemDark = true))
    }

    @Test
    fun explicitThemesIgnoreThePhone() {
        assertFalse(ThemeSetting.TRANSLATTE.isDark(systemDark = true))
        assertTrue(ThemeSetting.AMERICANO.isDark(systemDark = false))
    }

    @Test
    fun unknownOrMissingKeysFallBackToSystem() {
        assertEquals(ThemeSetting.SYSTEM, ThemeSetting.fromKey(null))
        assertEquals(ThemeSetting.SYSTEM, ThemeSetting.fromKey("mocha"))
        assertEquals(ThemeSetting.AMERICANO, ThemeSetting.fromKey("americano"))
    }
}
