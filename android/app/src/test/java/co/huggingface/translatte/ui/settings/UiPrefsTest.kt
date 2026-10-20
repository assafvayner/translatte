package co.huggingface.translatte.ui.settings

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class UiPrefsTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()

    @Test
    fun defaults() {
        val prefs = UiPrefs(context)
        assertEquals(ThemeSetting.SYSTEM, prefs.theme)
        assertFalse(prefs.resultHintSeen)
        assertFalse(prefs.cameraAsked)
    }

    @Test
    fun valuesPersistAcrossInstances() {
        UiPrefs(context).apply {
            theme = ThemeSetting.AMERICANO
            resultHintSeen = true
            cameraAsked = true
        }
        val reread = UiPrefs(context)
        assertEquals(ThemeSetting.AMERICANO, reread.theme)
        assertTrue(reread.resultHintSeen)
        assertTrue(reread.cameraAsked)
    }
}
