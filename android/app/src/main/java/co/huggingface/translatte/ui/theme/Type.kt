package co.huggingface.translatte.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Typography
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import co.huggingface.translatte.R

/** Fraunces 72pt: the wordmark, display and title sizes. */
val FrauncesDisplay = FontFamily(Font(R.font.fraunces_semibold, FontWeight.SemiBold))

/** Fraunces 9pt: everything from headline down. Static Fraunces has no Medium, so 500 resolves to Regular. */
val FrauncesText = FontFamily(
    Font(R.font.fraunces9pt_regular, FontWeight.Normal),
    Font(R.font.fraunces9pt_semibold, FontWeight.SemiBold),
)

/** The shared type tokens (design/DESIGN.md); iOS has the same names in `Typography`. */
object AppType {
    val wordmark = TextStyle(
        fontFamily = FrauncesDisplay,
        fontWeight = FontWeight.SemiBold,
        fontSize = 26.sp,
        lineHeight = 32.sp,
        letterSpacing = (-0.3).sp,
    )
    val display = TextStyle(fontFamily = FrauncesDisplay, fontWeight = FontWeight.SemiBold, fontSize = 24.sp, lineHeight = 30.sp)
    val title = TextStyle(fontFamily = FrauncesDisplay, fontWeight = FontWeight.SemiBold, fontSize = 20.sp, lineHeight = 26.sp)
    val headline = TextStyle(fontFamily = FrauncesText, fontWeight = FontWeight.SemiBold, fontSize = 17.sp, lineHeight = 22.sp)
    val body = TextStyle(fontFamily = FrauncesText, fontWeight = FontWeight.Normal, fontSize = 16.sp, lineHeight = 22.sp)
    val button = TextStyle(fontFamily = FrauncesText, fontWeight = FontWeight.SemiBold, fontSize = 16.sp, lineHeight = 20.sp)
    val label = TextStyle(fontFamily = FrauncesText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, lineHeight = 18.sp)
    val caption = TextStyle(fontFamily = FrauncesText, fontWeight = FontWeight.Normal, fontSize = 13.sp, lineHeight = 18.sp)

    /** Pass the text through [overlineText]: tokens can't carry the uppercase transform. */
    val overline = TextStyle(
        fontFamily = FrauncesText,
        fontWeight = FontWeight.SemiBold,
        fontSize = 11.sp,
        lineHeight = 14.sp,
        letterSpacing = 0.8.sp,
    )
}

fun overlineText(text: String) = text.uppercase()

/** Every M3 style maps to a token, so stock components (dialogs, sheets, buttons) render in Fraunces. */
val AppTypography = Typography(
    displayLarge = AppType.display,
    displayMedium = AppType.display,
    displaySmall = AppType.display,
    headlineLarge = AppType.title,
    headlineMedium = AppType.title,
    headlineSmall = AppType.title,
    titleLarge = AppType.title,
    titleMedium = AppType.headline,
    titleSmall = AppType.headline,
    bodyLarge = AppType.body,
    bodyMedium = AppType.body,
    bodySmall = AppType.caption,
    labelLarge = AppType.button,
    labelMedium = AppType.label,
    labelSmall = AppType.overline,
)

/** Shared radii and sizes (design/DESIGN.md). */
object Dimens {
    val cardRadius = 20.dp
    val innerRadius = 14.dp
    val sheetRadius = 28.dp
    val dialogRadius = 28.dp
    val buttonHeight = 56.dp
    val smallButtonHeight = 36.dp
    val chipHeight = 44.dp
    val rowMinHeight = 52.dp
    val gutter = 16.dp
    val spacing = 16.dp
}

val CardShape = RoundedCornerShape(Dimens.cardRadius)
val InnerShape = RoundedCornerShape(Dimens.innerRadius)
val SheetShape = RoundedCornerShape(topStart = Dimens.sheetRadius, topEnd = Dimens.sheetRadius)
val DialogShape = RoundedCornerShape(Dimens.dialogRadius)
