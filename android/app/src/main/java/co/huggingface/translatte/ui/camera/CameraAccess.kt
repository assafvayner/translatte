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
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
        AlertDialog(
            onDismissRequest = { help = false },
            title = { Text("Camera access is off") },
            text = { Text("translatte needs the camera to photograph text. Allow it in Settings, or choose a photo instead.") },
            confirmButton = {
                TextButton(onClick = {
                    help = false
                    context.startActivity(
                        Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", context.packageName, null)),
                    )
                }) { Text("Open Settings") }
            },
            dismissButton = { TextButton(onClick = { help = false }) { Text("Not now") } },
        )
    }
}

private tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}
