package co.huggingface.translatte.ui.camera

import android.graphics.Bitmap
import android.graphics.SurfaceTexture
import android.media.MediaMetadataRetriever
import android.media.MediaPlayer
import android.util.Log
import android.view.Surface
import android.view.TextureView
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameMillis
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.viewinterop.AndroidView
import java.nio.ByteBuffer
import kotlin.math.max
import kotlin.math.roundToInt

private const val TAG = "translatte"

/**
 * Debug builds: a video played in place of the camera, set from MainActivity's `mock_video` and
 * `mock_video_loop` extras. Its frames go to live translation as camera frames do.
 */
data class ReplaySource(val path: String, val loop: Boolean) {
    companion object {
        var current: ReplaySource? = null
    }
}

/**
 * Plays [source] at real time, aspect-filled like the camera preview, and offers [translator] the
 * frame on screen whenever it is free. Frames are grabbed upright (the player applies the clip's
 * rotation) with their long side within [LiveRules.MAX_FRAME_SIDE].
 */
@Composable
fun ReplayView(source: ReplaySource, translator: LiveTranslator, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val size = remember(source) { displaySize(source.path) }
    val textureView = remember { TextureView(context) }
    var player by remember { mutableStateOf<MediaPlayer?>(null) }
    DisposableEffect(source) {
        val mp = MediaPlayer()
        mp.isLooping = source.loop
        mp.setOnCompletionListener { Log.i(TAG, "live replay done") }
        fun play(texture: SurfaceTexture) {
            mp.setSurface(Surface(texture))
            mp.setDataSource(source.path)
            mp.prepare()
            mp.start()
            player = mp
        }
        textureView.surfaceTextureListener = object : TextureView.SurfaceTextureListener {
            override fun onSurfaceTextureAvailable(texture: SurfaceTexture, width: Int, height: Int) = play(texture)
            override fun onSurfaceTextureSizeChanged(texture: SurfaceTexture, width: Int, height: Int) {}
            override fun onSurfaceTextureDestroyed(texture: SurfaceTexture): Boolean = true
            override fun onSurfaceTextureUpdated(texture: SurfaceTexture) {}
        }
        textureView.surfaceTexture?.let(::play)
        onDispose {
            player = null
            textureView.surfaceTextureListener = null
            mp.release()
        }
    }
    LaunchedEffect(player) {
        val mp = player ?: return@LaunchedEffect
        val factor = LiveRules.downscaleFactor(size.first, size.second)
        val bitmap = Bitmap.createBitmap(size.first / factor, size.second / factor, Bitmap.Config.ARGB_8888)
        var started = false
        while (true) {
            withFrameMillis {}
            if (!mp.isPlaying) continue
            if (!started) {
                started = true
                translator.markStart(mp.currentPosition.toLong())
            }
            translator.offer {
                textureView.getBitmap(bitmap)
                val rgba = ByteArray(bitmap.byteCount)
                bitmap.copyPixelsToBuffer(ByteBuffer.wrap(rgba))
                LiveFrame(rgba, bitmap.width, bitmap.height, 0, mp.currentPosition.toLong())
            }
        }
    }
    Box(modifier.clipToBounds()) {
        AndroidView(
            factory = { textureView },
            modifier = Modifier.layout { measurable, constraints ->
                val scale = max(constraints.maxWidth.toFloat() / size.first, constraints.maxHeight.toFloat() / size.second)
                val w = (size.first * scale).roundToInt()
                val h = (size.second * scale).roundToInt()
                val placeable = measurable.measure(Constraints.fixed(w, h))
                layout(constraints.maxWidth, constraints.maxHeight) {
                    placeable.place((constraints.maxWidth - w) / 2, (constraints.maxHeight - h) / 2)
                }
            },
        )
    }
}

/** The clip's upright width and height. */
private fun displaySize(path: String): Pair<Int, Int> {
    val retriever = MediaMetadataRetriever()
    try {
        retriever.setDataSource(path)
        fun int(key: Int) = retriever.extractMetadata(key)?.toIntOrNull() ?: 0
        val w = int(MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH)
        val h = int(MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT)
        val rotation = int(MediaMetadataRetriever.METADATA_KEY_VIDEO_ROTATION)
        return if (rotation % 180 == 0) max(w, 1) to max(h, 1) else max(h, 1) to max(w, 1)
    } finally {
        retriever.release()
    }
}
