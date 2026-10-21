package co.huggingface.translatte

import android.content.ContentResolver
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.graphics.Matrix
import android.net.Uri
import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.ByteBuffer
import kotlin.math.max

object Images {
    /** Long-side cap for OCR; recognition gains little above this and memory grows quadratically. */
    private const val MAX_SIDE = 2048

    /** Decodes with EXIF orientation applied, so OCR coordinates match what's on screen. */
    fun load(resolver: ContentResolver, uri: Uri): Bitmap = decode(ImageDecoder.createSource(resolver, uri))

    fun load(file: File): Bitmap = decode(ImageDecoder.createSource(file))

    private fun decode(source: ImageDecoder.Source): Bitmap =
        ImageDecoder.decodeBitmap(source) { decoder, info, _ ->
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            val long = max(info.size.width, info.size.height)
            if (long > MAX_SIDE) {
                val s = MAX_SIDE.toFloat() / long
                decoder.setTargetSize((info.size.width * s).toInt(), (info.size.height * s).toInt())
            }
        }

    /** A bitmap of tightly packed RGBA rows, turned upright by rotating clockwise by [rotationDegrees]. */
    fun fromRgba(rgba: ByteArray, width: Int, height: Int, rotationDegrees: Int): Bitmap {
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(rgba))
        if (rotationDegrees == 0) return bitmap
        val rotate = Matrix().apply { postRotate(rotationDegrees.toFloat()) }
        return Bitmap.createBitmap(bitmap, 0, 0, width, height, rotate, true)
    }

    fun jpeg(bitmap: Bitmap): ByteArray =
        ByteArrayOutputStream().also { bitmap.compress(Bitmap.CompressFormat.JPEG, 92, it) }.toByteArray()
}
