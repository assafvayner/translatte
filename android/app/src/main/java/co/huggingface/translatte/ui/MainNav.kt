package co.huggingface.translatte.ui

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.ContextCompat
import co.huggingface.translatte.MainViewModel
import co.huggingface.translatte.Phase
import co.huggingface.translatte.ui.camera.CameraScreen
import co.huggingface.translatte.ui.home.HomeScreen
import co.huggingface.translatte.ui.home.LanguageBar
import co.huggingface.translatte.ui.home.directionLabel
import co.huggingface.translatte.ui.result.ResultScreen
import co.huggingface.translatte.ui.settings.SettingsScreen
import co.huggingface.translatte.ui.settings.ThemeSetting
import co.huggingface.translatte.ui.settings.UiPrefs

/** Everything after setup: camera, Home, result viewer, Languages and Settings. */
@Composable
fun MainNav(
    vm: MainViewModel,
    prefs: UiPrefs,
    theme: ThemeSetting,
    onTheme: (ThemeSetting) -> Unit,
    onScreenChange: (Screen) -> Unit,
    languagesScreen: @Composable (onBack: () -> Unit) -> Unit,
) {
    val context = LocalContext.current
    val hasCamera = remember { context.packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY) }
    fun cameraGranted() =
        ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED

    var screen by rememberSaveable { mutableStateOf(Nav.landing(hasCamera, cameraGranted(), prefs.cameraAsked)) }
    var resultOrigin by rememberSaveable { mutableStateOf(Screen.Home) }
    var cameraHelp by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(screen) { onScreenChange(screen) }
    val back: () -> Unit = { Nav.back(screen, resultOrigin)?.let { screen = it } }

    val gallery = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri: Uri? ->
        uri?.let {
            vm.translate(it)
            resultOrigin = Screen.Home
            screen = Screen.Result
        }
    }
    val cameraPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        prefs.cameraAsked = true
        if (ok) screen = Screen.Camera else cameraHelp = true
    }

    val direction = directionLabel(vm.sourceLanguages, vm.targetLanguages, vm.sourceCode, vm.targetCode)
    val languageBar = @Composable {
        LanguageBar(
            vm.sourceLanguages,
            vm.targetLanguages,
            vm.sourceCode,
            vm.targetCode,
            vm::setSource,
            vm::setTarget,
            vm::swap,
        )
    }

    when (screen) {
        Screen.Camera -> CameraScreen(
            direction = direction,
            languageBar = languageBar,
            onBack = back,
            onCaptured = { file ->
                vm.translate(file)
                resultOrigin = Screen.Camera
                screen = Screen.Result
            },
            onPermissionResult = { ok ->
                prefs.cameraAsked = true
                if (!ok) screen = Screen.Home
            },
        )
        Screen.Home -> HomeScreen(
            languageBar = languageBar,
            lastImage = vm.image,
            lastResult = vm.result,
            working = vm.phase is Phase.Working,
            memoryWarning = vm.memoryWarning,
            hasCamera = hasCamera,
            onCamera = {
                if (cameraGranted()) screen = Screen.Camera else cameraPermission.launch(Manifest.permission.CAMERA)
            },
            onPhotos = { gallery.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) },
            onOpenResult = {
                resultOrigin = Screen.Home
                screen = Screen.Result
            },
            onLanguages = { screen = Screen.Languages },
            onSettings = { screen = Screen.Settings },
        )
        Screen.Result -> {
            if (vm.image == null && vm.phase !is Phase.Working) {
                LaunchedEffect(Unit) { back() }
            } else {
                ResultScreen(
                    image = vm.image,
                    result = vm.result,
                    phase = vm.phase,
                    direction = direction,
                    hintSeen = prefs.resultHintSeen,
                    onHintSeen = { prefs.resultHintSeen = true },
                    onRetry = vm::retry,
                    onBack = back,
                )
            }
        }
        Screen.Languages -> {
            BackHandler(onBack = back)
            languagesScreen(back)
        }
        Screen.Settings -> SettingsScreen(theme, onTheme, onBack = back)
    }

    if (cameraHelp) {
        AlertDialog(
            onDismissRequest = { cameraHelp = false },
            title = { Text("Camera access is off") },
            text = { Text("translatte needs the camera to photograph text. Allow it in Settings, or choose a photo instead.") },
            confirmButton = {
                TextButton(onClick = {
                    cameraHelp = false
                    context.startActivity(
                        Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", context.packageName, null)),
                    )
                }) { Text("Open Settings") }
            },
            dismissButton = { TextButton(onClick = { cameraHelp = false }) { Text("Not now") } },
        )
    }
}
