package co.huggingface.translatte.ui.settings

import android.app.UiModeManager
import android.content.Context
import uniffi.translatte_ffi.OcrEngine

enum class ThemeSetting(val key: String, val label: String, val description: String) {
    SYSTEM("system", "System", "translatte in light mode, americano in dark mode"),
    TRANSLATTE("translatte", "translatte", "Light"),
    AMERICANO("americano", "americano", "Dark"),
    ;

    fun isDark(systemDark: Boolean): Boolean = when (this) {
        SYSTEM -> systemDark
        TRANSLATTE -> false
        AMERICANO -> true
    }

    /** For [UiModeManager.setApplicationNightMode], so the splash and window follow an override. */
    val nightMode: Int
        get() = when (this) {
            SYSTEM -> UiModeManager.MODE_NIGHT_AUTO
            TRANSLATTE -> UiModeManager.MODE_NIGHT_NO
            AMERICANO -> UiModeManager.MODE_NIGHT_YES
        }

    companion object {
        fun fromKey(key: String?): ThemeSetting = entries.firstOrNull { it.key == key } ?: SYSTEM
    }
}

/** UI preferences, in the same SharedPreferences file as the language choice. */
class UiPrefs(context: Context) {
    private val prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE)

    var theme: ThemeSetting
        get() = ThemeSetting.fromKey(prefs.getString(THEME, null))
        set(value) = prefs.edit().putString(THEME, value.key).apply()

    /** The result viewer's gesture hint hides for good after the first gesture. */
    var resultHintSeen: Boolean
        get() = prefs.getBoolean(RESULT_HINT_SEEN, false)
        set(value) = prefs.edit().putBoolean(RESULT_HINT_SEEN, value).apply()

    /** Camera permission was requested once; after a denial the app lands on Home. */
    var cameraAsked: Boolean
        get() = prefs.getBoolean(CAMERA_ASKED, false)
        set(value) = prefs.edit().putBoolean(CAMERA_ASKED, value).apply()

    /** Live camera translation (experimental); on by default in this build. */
    var liveTranslation: Boolean
        get() = prefs.getBoolean(LIVE_TRANSLATION, true)
        set(value) = prefs.edit().putBoolean(LIVE_TRANSLATION, value).apply()

    /** Debug builds only; see [ocrEngineFromKey]. */
    var ocrEngine: OcrEngine
        get() = ocrEngineFromKey(prefs.getString(OCR_ENGINE, null))
        set(value) = prefs.edit().putString(OCR_ENGINE, value.key).apply()

    private companion object {
        const val THEME = "theme"
        const val RESULT_HINT_SEEN = "result_hint_seen"
        const val CAMERA_ASKED = "camera_asked"
        const val LIVE_TRANSLATION = "live_translation"
        const val OCR_ENGINE = "ocr_engine"
    }
}
