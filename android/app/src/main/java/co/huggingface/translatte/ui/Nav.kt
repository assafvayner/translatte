package co.huggingface.translatte.ui

/** Screens after setup. Setup itself is shown whenever required models are missing. */
enum class Screen { Camera, Home, Result, Languages, Settings }

object Nav {
    /** The camera is the landing screen unless there is none or its permission was already denied. */
    fun landing(hasCamera: Boolean, cameraGranted: Boolean, cameraAsked: Boolean): Screen =
        if (hasCamera && (cameraGranted || !cameraAsked)) Screen.Camera else Screen.Home

    /** Where back goes from [current]; null leaves the app. */
    fun back(current: Screen, resultOrigin: Screen): Screen? = when (current) {
        Screen.Home -> null
        Screen.Result -> resultOrigin
        Screen.Camera, Screen.Languages, Screen.Settings -> Screen.Home
    }
}
