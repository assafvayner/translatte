package co.huggingface.translatte

import org.junit.Assert.assertEquals
import org.junit.Test

class SizesTest {
    @Test
    fun megabytes() {
        assertEquals("0.0 MB", formatSize(0))
        assertEquals("4.8 MB", formatSize(4_800_000))
        assertEquals("8.0 MB", formatSize(8_000_000))
        assertEquals("16.6 MB", formatSize(16_600_000))
        assertEquals("13.5 MB", formatSize(13_480_000))
        assertEquals("512 MB", formatSize(512_000_000))
        assertEquals("999 MB", formatSize(999_400_000))
    }

    @Test
    fun gigabytes() {
        assertEquals("1.00 GB", formatSize(999_600_000))
        assertEquals("2.49 GB", formatSize(2_490_000_000))
        assertEquals("2.52 GB", formatSize(2_518_000_000))
        assertEquals("20.7 GB", formatSize(20_700_000_000))
        assertEquals("207 GB", formatSize(207_380_000_000))
    }
}
