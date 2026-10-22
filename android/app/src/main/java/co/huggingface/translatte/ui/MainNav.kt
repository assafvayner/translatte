package co.huggingface.translatte.ui

import android.content.pm.PackageManager
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import co.huggingface.translatte.MainViewModel
import co.huggingface.translatte.Phase
import co.huggingface.translatte.ui.camera.CameraAccess
import co.huggingface.translatte.ui.camera.CameraScreen
import co.huggingface.translatte.ui.camera.ReplaySource
import co.huggingface.translatte.ui.camera.LiveRules
import co.huggingface.translatte.ui.camera.cameraGranted
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
    var screen by rememberSaveable {
        mutableStateOf(
            if (ReplaySource.current != null) Screen.Camera else Nav.landing(hasCamera, cameraGranted(context), prefs.cameraAsked),
        )
    }
    var resultOrigin by rememberSaveable { mutableStateOf(Screen.Home) }
    var hintSeen by remember { mutableStateOf(prefs.resultHintSeen) }
    var live by remember { mutableStateOf(prefs.liveTranslation) }
    LaunchedEffect(screen) { onScreenChange(screen) }
    val back: () -> Unit = { Nav.back(screen, resultOrigin)?.let { screen = it } }

    val gallery = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri: Uri? ->
        uri?.let {
            vm.translate(it)
            resultOrigin = Screen.Home
            screen = Screen.Result
        }
    }

    val direction = directionLabel(vm.sourceLanguages, vm.sourceCode, vm.target)
    val languageBar = @Composable {
        LanguageBar(vm.sourceLanguages, vm.target, vm.sourceCode, vm::setSource)
    }

    when (screen) {
        Screen.Camera -> CameraScreen(
            direction = direction,
            languageBar = languageBar,
            live = LiveRules.isActive(live, vm.sourceLanguages.any { it.code == vm.sourceCode }),
            sourceCode = vm.sourceCode,
            readFrame = vm::readFrame,
            translateRegions = vm::translateRegions,
            onLiveSnap = { bitmap, result ->
                vm.showLive(bitmap, result)
                resultOrigin = Screen.Camera
                screen = Screen.Result
            },
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
        Screen.Home -> CameraAccess(prefs, onOpen = { screen = Screen.Camera }) { onCamera ->
            HomeScreen(
                languageBar = languageBar,
                lastImage = vm.lastSuccess?.image,
                lastResult = vm.lastSuccess?.result,
                working = vm.phase is Phase.Working,
                memoryWarning = vm.memoryWarning,
                hasCamera = hasCamera,
                onCamera = onCamera,
                onPhotos = { gallery.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) },
                onOpenResult = {
                    vm.reopenLast()
                    resultOrigin = Screen.Home
                    screen = Screen.Result
                },
                onLanguages = { screen = Screen.Languages },
                onSettings = { screen = Screen.Settings },
            )
        }
        Screen.Result -> {
            if (!Nav.hasResult(vm.phase)) {
                LaunchedEffect(Unit) { back() }
            } else {
                ResultScreen(
                    image = vm.image,
                    result = vm.result,
                    phase = vm.phase,
                    direction = direction,
                    hintSeen = hintSeen,
                    onHintSeen = {
                        hintSeen = true
                        prefs.resultHintSeen = true
                    },
                    onBack = back,
                )
            }
        }
        Screen.Languages -> {
            BackHandler(onBack = back)
            languagesScreen(back)
        }
        Screen.Settings -> SettingsScreen(
            theme,
            onTheme,
            onBack = back,
            live = live,
            onLive = {
                live = it
                prefs.liveTranslation = it
            },
        )
    }
}
