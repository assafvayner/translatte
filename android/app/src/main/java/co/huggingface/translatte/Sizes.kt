package co.huggingface.translatte

import java.util.Locale

/**
 * Decimal units, matching iOS: MB below 1000 MB, else GB. Under 10 shows one decimal for MB and two
 * for GB, under 100 one decimal, and none from 100 up ("4.8 MB", "2.49 GB", "16.6 MB", "207 GB").
 */
fun formatSize(bytes: Long): String {
    val mb = bytes / 1e6
    val (value, unit) = if (mb < 999.5) mb to "MB" else bytes / 1e9 to "GB"
    val decimals = when {
        value < 10 -> if (unit == "GB") 2 else 1
        value < 100 -> 1
        else -> 0
    }
    return String.format(Locale.US, "%.${decimals}f %s", value, unit)
}
