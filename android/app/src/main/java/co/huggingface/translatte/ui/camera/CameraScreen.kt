package co.huggingface.translatte.ui.camera

import android.Manifest
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.os.PowerManager
import android.util.Size
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.Camera
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageCapture
import androidx.camera.core.ImageCaptureException
import androidx.camera.core.Preview
import androidx.camera.core.ZoomState
import androidx.camera.core.resolutionselector.AspectRatioStrategy
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
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
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
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
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.Observer
import androidx.lifecycle.compose.LocalLifecycleOwner
import co.huggingface.translatte.ui.theme.AppType
import co.huggingface.translatte.ui.theme.Dimens
import co.huggingface.translatte.ui.theme.InnerShape
import co.huggingface.translatte.ui.theme.SheetShape
import uniffi.translatte_ffi.TranslationResult
import java.io.File
import java.util.concurrent.Executors

/** Fixed espresso scrims and cream text, the same in both themes and on iOS. */
private val Scrim = Color(0x9916110E)
private val OnScrim = Color(0xFFF1E6D8)
private val ScrimEdge = Color(0x8C16110E)

/** Preview and live frames share one aspect ratio, so the live overlay lines up with the preview. */
private val FourByThree = ResolutionSelector.Builder()
    .setAspectRatioStrategy(AspectRatioStrategy.RATIO_4_3_FALLBACK_AUTO_STRATEGY)
    .build()

/**
 * The landing screen: a CameraX viewfinder with controls on a fixed dark scrim in either theme.
 * Binding follows the activity lifecycle, so the camera stops in the background, and unbinds when
 * the screen is left. With [live], frames are translated continuously and painted over the preview.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CameraScreen(
    direction: String,
    languageBar: @Composable () -> Unit,
    live: Boolean,
    sourceCode: String,
    translateFrame: suspend (LiveFrame) -> TranslationResult,
    onLiveSnap: (Bitmap, TranslationResult) -> Unit,
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
    var flash by remember { mutableStateOf(false) }
    var twoX by remember { mutableStateOf(false) }
    var maxZoom by remember { mutableFloatStateOf(1f) }
    var error by remember { mutableStateOf<String?>(null) }
    var sheet by remember { mutableStateOf(false) }

    val scope = rememberCoroutineScope()
    val liveTranslator = remember {
        val power = context.getSystemService(PowerManager::class.java)
        LiveTranslator(scope, translateFrame) { power.currentThermalStatus >= PowerManager.THERMAL_STATUS_SEVERE }
    }
    val analysisExecutor = remember { Executors.newSingleThreadExecutor() }
    val analysis = remember {
        ImageAnalysis.Builder()
            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .setOutputImageFormat(ImageAnalysis.OUTPUT_IMAGE_FORMAT_RGBA_8888)
            .setResolutionSelector(
                ResolutionSelector.Builder()
                    .setAspectRatioStrategy(AspectRatioStrategy.RATIO_4_3_FALLBACK_AUTO_STRATEGY)
                    .setResolutionStrategy(
                        ResolutionStrategy(Size(960, 720), ResolutionStrategy.FALLBACK_RULE_CLOSEST_HIGHER_THEN_LOWER),
                    )
                    .build(),
            )
            .build()
            .also { it.setAnalyzer(analysisExecutor, liveTranslator) }
    }
    DisposableEffect(Unit) {
        onDispose {
            analysis.clearAnalyzer()
            analysisExecutor.shutdown()
        }
    }
    LaunchedEffect(live) {
        liveTranslator.enabled = live
        if (!live) liveTranslator.reset()
    }
    LaunchedEffect(sourceCode) { liveTranslator.reset() }

    LaunchedEffect(granted) {
        if (!granted) return@LaunchedEffect
        val future = ProcessCameraProvider.getInstance(context)
        future.addListener(
            {
                provider = try {
                    future.get()
                } catch (e: Exception) {
                    error = "Couldn't start the camera."
                    null
                }
            },
            ContextCompat.getMainExecutor(context),
        )
    }
    DisposableEffect(provider, lifecycleOwner, live) {
        val p = provider ?: return@DisposableEffect onDispose {}
        val preview = Preview.Builder().setResolutionSelector(FourByThree).build()
            .also { it.surfaceProvider = previewView.surfaceProvider }
        val useCases = listOfNotNull(preview, capture, analysis.takeIf { live })
        p.unbindAll()
        camera = try {
            p.bindToLifecycle(lifecycleOwner, CameraSelector.DEFAULT_BACK_CAMERA, *useCases.toTypedArray())
        } catch (e: IllegalArgumentException) {
            error = "No back camera available."
            null
        } catch (e: Exception) {
            error = "Couldn't start the camera."
            null
        }
        onDispose {
            p.unbindAll()
            camera = null
        }
    }
    LaunchedEffect(flash) { capture.flashMode = if (flash) ImageCapture.FLASH_MODE_ON else ImageCapture.FLASH_MODE_OFF }
    DisposableEffect(camera, lifecycleOwner) {
        val zoomState = camera?.cameraInfo?.zoomState ?: return@DisposableEffect onDispose {}
        val observer = Observer<ZoomState> { maxZoom = it.maxZoomRatio }
        zoomState.observe(lifecycleOwner, observer)
        onDispose { zoomState.removeObserver(observer) }
    }
    LaunchedEffect(twoX, camera, maxZoom) {
        camera?.cameraControl?.setZoomRatio(if (twoX) minOf(2f, maxZoom) else 1f)
    }

    fun shoot() {
        error = null
        val dir = File(context.cacheDir, "photos").apply { mkdirs() }
        val file = File.createTempFile("capture", ".jpg", dir)
        capture.takePicture(
            ImageCapture.OutputFileOptions.Builder(file).build(),
            ContextCompat.getMainExecutor(context),
            object : ImageCapture.OnImageSavedCallback {
                override fun onImageSaved(output: ImageCapture.OutputFileResults) = onCaptured(file)

                override fun onError(e: ImageCaptureException) {
                    file.delete()
                    error = "Couldn't take the photo: ${e.message}"
                }
            },
        )
    }

    fun snap() {
        val frozen = liveTranslator.snapshot()
        if (frozen != null) onLiveSnap(frozen.first, frozen.second) else shoot()
    }

    Box(Modifier.fillMaxSize().background(Color.Black)) {
        if (granted) AndroidView(factory = { previewView }, modifier = Modifier.fillMaxSize())
        if (granted && live) {
            LiveOverlay(liveTranslator.overlay, liveTranslator.imageSize, Modifier.fillMaxSize())
        }
        liveTranslator.readout?.let {
            Surface(
                shape = InnerShape,
                color = Scrim,
                contentColor = OnScrim,
                modifier = Modifier.align(Alignment.TopStart).windowInsetsPadding(WindowInsets.safeDrawing)
                    .padding(start = 16.dp, top = 64.dp),
            ) {
                Text(it, Modifier.padding(horizontal = 8.dp, vertical = 4.dp), style = AppType.caption)
            }
        }
        val statusBar = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
        val navBar = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
        Box(
            Modifier.fillMaxWidth().height(statusBar + 96.dp)
                .background(Brush.verticalGradient(listOf(ScrimEdge, Color.Transparent))),
        )
        Box(
            Modifier.align(Alignment.BottomCenter).fillMaxWidth().height(navBar + 160.dp)
                .background(Brush.verticalGradient(listOf(Color.Transparent, ScrimEdge))),
        )

        Row(
            Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing).padding(horizontal = 16.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            ScrimIconButton(Icons.AutoMirrored.Filled.ArrowBack, "Back to Home", onBack)
            Surface(
                shape = CircleShape,
                color = Scrim,
                contentColor = OnScrim,
                modifier = Modifier.clip(CircleShape).clickable(onClickLabel = "Change languages") {
                    sheet = true
                },
            ) {
                Box(Modifier.height(Dimens.chipHeight).padding(horizontal = 16.dp), contentAlignment = Alignment.Center) {
                    Text(direction, style = AppType.label, maxLines = 1)
                }
            }
            Spacer(Modifier.weight(1f))
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
                Surface(shape = InnerShape, color = Scrim, contentColor = OnScrim) {
                    Text(
                        it,
                        Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                        style = AppType.caption,
                    )
                }
            }
            Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                Box(
                    Modifier
                        .size(80.dp)
                        .border(4.dp, OnScrim, CircleShape)
                        .padding(7.dp)
                        .clip(CircleShape)
                        .background(MaterialTheme.colorScheme.primary)
                        .clickable { snap() }
                        .semantics { contentDescription = "Take photo" },
                )
                Surface(
                    shape = CircleShape,
                    color = Scrim,
                    contentColor = OnScrim,
                    modifier = Modifier.align(Alignment.CenterEnd).padding(end = 28.dp).size(52.dp).clip(CircleShape)
                        .clickable(onClickLabel = "Toggle zoom") { twoX = !twoX },
                ) {
                    Box(contentAlignment = Alignment.Center) {
                        Text(if (twoX) "2×" else "1×", style = AppType.label)
                    }
                }
            }
        }
    }

    if (sheet) {
        ModalBottomSheet(
            onDismissRequest = { sheet = false },
            shape = SheetShape,
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ) {
            Column(Modifier.padding(horizontal = Dimens.gutter).navigationBarsPadding()) {
                Text(
                    "Languages",
                    style = AppType.title,
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
