package co.huggingface.translatte.ui

import co.huggingface.translatte.Phase

/** Screens after setup. Setup itself is shown whenever required models are missing. */
enum class Screen { Camera, Home, Result, Languages, Settings }

/** What Home's Camera button does. */
enum class CameraTap { Open, Request, Explain }

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

    /** Once the permission was denied, explain and link to Settings rather than prompting again. */
    fun cameraTap(granted: Boolean, asked: Boolean, showRationale: Boolean): CameraTap = when {
        granted -> CameraTap.Open
        asked || showRationale -> CameraTap.Explain
        else -> CameraTap.Request
    }

    /** The result viewer has something to show (a photo, its progress or its error) unless nothing ran yet. */
    fun hasResult(phase: Phase): Boolean = phase !is Phase.Idle
}
