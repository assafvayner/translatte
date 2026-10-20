package co.huggingface.translatte.ui.camera

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import co.huggingface.translatte.ui.CameraTap
import co.huggingface.translatte.ui.Nav
import co.huggingface.translatte.ui.common.AppDialog
import co.huggingface.translatte.ui.settings.UiPrefs

fun cameraGranted(context: Context): Boolean =
    ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED

/**
 * Home's Camera button: [content] gets the click handler. It opens the camera when allowed, asks
 * for the permission the first time, and after a denial explains and links to Settings instead of
 * prompting again.
 */
@Composable
fun CameraAccess(prefs: UiPrefs, onOpen: () -> Unit, content: @Composable (onCamera: () -> Unit) -> Unit) {
    val context = LocalContext.current
    var help by rememberSaveable { mutableStateOf(false) }
    val request = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        prefs.cameraAsked = true
        if (ok) onOpen() else help = true
    }
    content {
        val rationale = context.findActivity()?.let {
            ActivityCompat.shouldShowRequestPermissionRationale(it, Manifest.permission.CAMERA)
        } ?: false
        when (Nav.cameraTap(cameraGranted(context), prefs.cameraAsked, rationale)) {
            CameraTap.Open -> onOpen()
            CameraTap.Request -> request.launch(Manifest.permission.CAMERA)
            CameraTap.Explain -> help = true
        }
    }
    if (help) {
        AppDialog(
            title = "Camera access is off",
            message = "translatte needs the camera to photograph text. Allow it in Settings, or choose a photo instead.",
            onDismiss = { help = false },
            confirm = "Open Settings" to {
                help = false
                context.startActivity(
                    Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", context.packageName, null)),
                )
            },
            dismiss = listOf("Not now" to { help = false }),
        )
    }
}

private tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}
