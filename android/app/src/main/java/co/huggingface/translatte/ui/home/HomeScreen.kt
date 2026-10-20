package co.huggingface.translatte.ui.home

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.PhotoCamera
import androidx.compose.material.icons.outlined.Language
import androidx.compose.material.icons.outlined.PhotoLibrary
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import co.huggingface.translatte.R
import co.huggingface.translatte.ui.common.NoticeBanner
import co.huggingface.translatte.ui.result.OverlayImage
import co.huggingface.translatte.ui.theme.DisplayTitleStyle
import co.huggingface.translatte.ui.theme.WordmarkStyle
import uniffi.translatte_ffi.TranslationResult

@Composable
fun HomeScreen(
    languageBar: @Composable () -> Unit,
    lastImage: Bitmap?,
    lastResult: TranslationResult?,
    working: Boolean,
    memoryWarning: String?,
    hasCamera: Boolean,
    onCamera: () -> Unit,
    onPhotos: () -> Unit,
    onOpenResult: () -> Unit,
    onLanguages: () -> Unit,
    onSettings: () -> Unit,
) {
    Scaffold { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Header(working, onLanguages, onSettings)
            languageBar()
            if (lastImage != null) {
                LastResultCard(lastImage, lastResult, working, onOpenResult, Modifier.weight(1f).fillMaxWidth())
            } else {
                EmptyState(Modifier.weight(1f).fillMaxWidth())
            }
            memoryWarning?.let { NoticeBanner(it) }
            Row(Modifier.padding(bottom = 8.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                if (hasCamera) {
                    Button(onClick = onCamera, enabled = !working, modifier = Modifier.weight(1f).height(56.dp)) {
                        Icon(Icons.Filled.PhotoCamera, contentDescription = null)
                        Spacer(Modifier.width(8.dp))
                        Text("Camera")
                    }
                }
                FilledTonalButton(onClick = onPhotos, enabled = !working, modifier = Modifier.weight(1f).height(56.dp)) {
                    Icon(Icons.Outlined.PhotoLibrary, contentDescription = null)
                    Spacer(Modifier.width(8.dp))
                    Text("Photos")
                }
            }
        }
    }
}

@Composable
private fun Header(working: Boolean, onLanguages: () -> Unit, onSettings: () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Image(painterResource(R.drawable.logo_cup), contentDescription = null, modifier = Modifier.size(34.dp))
        Spacer(Modifier.width(8.dp))
        Text(
            "translatte",
            style = WordmarkStyle,
            color = MaterialTheme.colorScheme.onBackground,
            modifier = Modifier.weight(1f),
        )
        FilledTonalButton(
            onClick = onLanguages,
            enabled = !working,
            contentPadding = PaddingValues(horizontal = 14.dp, vertical = 6.dp),
        ) {
            Icon(Icons.Outlined.Language, contentDescription = null, modifier = Modifier.size(18.dp))
            Spacer(Modifier.width(6.dp))
            Text("Languages")
        }
        IconButton(onClick = onSettings) { Icon(Icons.Outlined.Settings, contentDescription = "Settings") }
    }
}

@Composable
private fun EmptyState(modifier: Modifier) {
    val outline = MaterialTheme.colorScheme.outline
    Column(
        modifier
            .drawBehind {
                drawRoundRect(
                    outline,
                    cornerRadius = CornerRadius(24.dp.toPx()),
                    style = Stroke(
                        width = 1.5.dp.toPx(),
                        pathEffect = PathEffect.dashPathEffect(floatArrayOf(8.dp.toPx(), 6.dp.toPx())),
                    ),
                )
            }
            .padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
    ) {
        Image(painterResource(R.drawable.logo_cup), contentDescription = null, modifier = Modifier.size(120.dp))
        Text(
            "Point at a sign, menu or label",
            style = DisplayTitleStyle,
            color = MaterialTheme.colorScheme.onBackground,
            textAlign = TextAlign.Center,
        )
        Text(
            "Take a photo and the translation appears right on top of the text. Works fully offline.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun LastResultCard(
    image: Bitmap,
    result: TranslationResult?,
    working: Boolean,
    onOpen: () -> Unit,
    modifier: Modifier,
) {
    Card(
        onClick = onOpen,
        modifier = modifier.semantics { contentDescription = "Last result" },
        shape = RoundedCornerShape(24.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Box(Modifier.fillMaxSize()) {
            OverlayImage(image, result, showOriginal = false, modifier = Modifier.fillMaxSize())
            if (working) LinearProgressIndicator(Modifier.fillMaxWidth().align(Alignment.BottomCenter))
        }
    }
}
