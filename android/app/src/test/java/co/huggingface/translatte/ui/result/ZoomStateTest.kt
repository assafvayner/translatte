package co.huggingface.translatte.ui.result

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class ZoomStateTest {
    private val viewport = Size(1000f, 2000f)
    private val zoom = ZoomState()

    @Before
    fun setUp() {
        zoom.setBounds(viewport, fitRect(1000, 1000, viewport))
    }

    @Test
    fun fitRectCentersThePhoto() {
        assertEquals(Rect(0f, 500f, 1000f, 1500f), fitRect(1000, 1000, viewport))
        assertEquals(Rect(250f, 0f, 750f, 2000f), fitRect(500, 2000, viewport))
    }

    @Test
    fun startsFitted() {
        assertEquals(1f, zoom.scale)
        assertEquals(Offset.Zero, zoom.offset)
        assertFalse(zoom.zoomed)
    }

    @Test
    fun doubleTapZoomsAroundTheTapThenResets() {
        zoom.doubleTap(Offset(500f, 1000f))
        assertEquals(2.5f, zoom.scale)
        assertEquals(Offset(-750f, -1500f), zoom.offset)
        assertTrue(zoom.zoomed)

        zoom.doubleTap(Offset(100f, 100f))
        assertEquals(1f, zoom.scale)
        assertEquals(Offset.Zero, zoom.offset)
        assertFalse(zoom.zoomed)
    }

    @Test
    fun pinchIsClampedBetweenOneAndSix() {
        zoom.transform(Offset(500f, 1000f), Offset.Zero, 10f)
        assertEquals(6f, zoom.scale)
        zoom.transform(Offset(500f, 1000f), Offset.Zero, 0.01f)
        assertEquals(1f, zoom.scale)
    }

    @Test
    fun panIsClampedToThePhotoEdges() {
        zoom.transform(Offset(500f, 1000f), Offset(300f, 0f), 1f)
        assertEquals(Offset.Zero, zoom.offset)

        zoom.doubleTap(Offset(500f, 1000f))
        zoom.transform(Offset(500f, 1000f), Offset(10_000f, 10_000f), 1f)
        assertEquals(Offset(0f, -1250f), zoom.offset)
        zoom.transform(Offset(500f, 1000f), Offset(-10_000f, -10_000f), 1f)
        assertEquals(Offset(-1500f, -1750f), zoom.offset)
    }
}
