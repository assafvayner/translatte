package co.huggingface.translatte.ui.camera

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.Camera
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageCapture
import androidx.camera.core.ImageCaptureException
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.FlashOff
import androidx.compose.material.icons.filled.FlashOn
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LocalLifecycleOwner
import java.io.File

private val Scrim = Color(0x8C000000)
private val OnScrim = Color.White

/**
 * The landing screen: a CameraX viewfinder with controls on a fixed dark scrim in either theme.
 * Binding follows the activity lifecycle, so the camera stops in the background, and unbinds when
 * the screen is left.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CameraScreen(
    direction: String,
    languageBar: @Composable () -> Unit,
    onBack: () -> Unit,
    onCaptured: (File) -> Unit,
    onPermissionResult: (granted: Boolean) -> Unit,
) {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    var granted by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED,
        )
    }
    val request = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        granted = ok
        onPermissionResult(ok)
    }
    LaunchedEffect(Unit) { if (!granted) request.launch(Manifest.permission.CAMERA) }
    BackHandler(onBack = onBack)

    val previewView = remember { PreviewView(context).apply { scaleType = PreviewView.ScaleType.FILL_CENTER } }
    val capture = remember {
        ImageCapture.Builder().setCaptureMode(ImageCapture.CAPTURE_MODE_MAXIMIZE_QUALITY).build()
    }
    var provider by remember { mutableStateOf<ProcessCameraProvider?>(null) }
    var camera by remember { mutableStateOf<Camera?>(null) }
    var flash by rememberSaveable { mutableStateOf(false) }
    var twoX by rememberSaveable { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var sheet by remember { mutableStateOf(false) }

    LaunchedEffect(granted) {
        if (!granted) return@LaunchedEffect
        val future = ProcessCameraProvider.getInstance(context)
        future.addListener({ provider = future.get() }, ContextCompat.getMainExecutor(context))
    }
    DisposableEffect(provider, lifecycleOwner) {
        val p = provider ?: return@DisposableEffect onDispose {}
        val preview = Preview.Builder().build().also { it.surfaceProvider = previewView.surfaceProvider }
        p.unbindAll()
        camera = try {
            p.bindToLifecycle(lifecycleOwner, CameraSelector.DEFAULT_BACK_CAMERA, preview, capture)
        } catch (e: IllegalArgumentException) {
            error = "No back camera available."
            null
        }
        onDispose {
            p.unbindAll()
            camera = null
        }
    }
    LaunchedEffect(flash) { capture.flashMode = if (flash) ImageCapture.FLASH_MODE_ON else ImageCapture.FLASH_MODE_OFF }
    LaunchedEffect(twoX, camera) {
        val c = camera ?: return@LaunchedEffect
        val max = c.cameraInfo.zoomState.value?.maxZoomRatio ?: 1f
        c.cameraControl.setZoomRatio(if (twoX) minOf(2f, max) else 1f)
    }

    fun shoot() {
        busy = true
        error = null
        val file = File(context.cacheDir, "photos/capture.jpg").apply { parentFile?.mkdirs() }
        capture.takePicture(
            ImageCapture.OutputFileOptions.Builder(file).build(),
            ContextCompat.getMainExecutor(context),
            object : ImageCapture.OnImageSavedCallback {
                override fun onImageSaved(output: ImageCapture.OutputFileResults) {
                    busy = false
                    onCaptured(file)
                }

                override fun onError(e: ImageCaptureException) {
                    busy = false
                    error = "Couldn't take the photo: ${e.message}"
                }
            },
        )
    }

    Box(Modifier.fillMaxSize().background(Color.Black)) {
        if (granted) AndroidView(factory = { previewView }, modifier = Modifier.fillMaxSize())

        Row(
            Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing).padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            ScrimIconButton(Icons.Filled.Close, "Back to Home", onBack)
            Surface(
                shape = RoundedCornerShape(50),
                color = Scrim,
                contentColor = OnScrim,
                modifier = Modifier.clip(RoundedCornerShape(50)).clickable(onClickLabel = "Change languages") {
                    sheet = true
                },
            ) {
                Text(
                    direction,
                    Modifier.padding(horizontal = 16.dp, vertical = 10.dp),
                    style = MaterialTheme.typography.labelLarge,
                )
            }
            ScrimIconButton(
                if (flash) Icons.Filled.FlashOn else Icons.Filled.FlashOff,
                if (flash) "Flash on" else "Flash off",
            ) { flash = !flash }
        }

        Column(
            Modifier.align(Alignment.BottomCenter).fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing)
                .padding(bottom = 24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            error?.let {
                Surface(shape = RoundedCornerShape(12.dp), color = Scrim, contentColor = OnScrim) {
                    Text(
                        it,
                        Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
            Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                Box(
                    Modifier
                        .size(76.dp)
                        .border(4.dp, OnScrim, CircleShape)
                        .padding(7.dp)
                        .clip(CircleShape)
                        .background(MaterialTheme.colorScheme.primary)
                        .clickable(enabled = !busy && camera != null) { shoot() }
                        .semantics { contentDescription = "Take photo" },
                )
                Surface(
                    shape = CircleShape,
                    color = Scrim,
                    contentColor = OnScrim,
                    modifier = Modifier.align(Alignment.CenterEnd).padding(end = 32.dp).size(48.dp).clip(CircleShape)
                        .clickable(onClickLabel = "Toggle zoom") { twoX = !twoX },
                ) {
                    Box(contentAlignment = Alignment.Center) {
                        Text(if (twoX) "2×" else "1×", style = MaterialTheme.typography.labelLarge)
                    }
                }
            }
        }
    }

    if (sheet) {
        ModalBottomSheet(onDismissRequest = { sheet = false }) {
            Column(Modifier.padding(horizontal = 16.dp).navigationBarsPadding()) {
                Text(
                    "Languages",
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.padding(bottom = 12.dp),
                )
                languageBar()
                Spacer(Modifier.height(24.dp))
            }
        }
    }
}

@Composable
private fun ScrimIconButton(icon: ImageVector, description: String, onClick: () -> Unit) {
    Surface(shape = CircleShape, color = Scrim, contentColor = OnScrim) {
        IconButton(onClick = onClick) { Icon(icon, contentDescription = description) }
    }
}
