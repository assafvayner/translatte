package co.huggingface.translatte.ui.setup

import android.os.StatFs
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.outlined.Circle
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import co.huggingface.translatte.DownloadState
import co.huggingface.translatte.R
import co.huggingface.translatte.formatSize
import co.huggingface.translatte.ui.theme.DisplayTitleStyle
import co.huggingface.translatte.ui.theme.appBarColors
import uniffi.translatte_ffi.ModelInfo
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.translatte_ffi.ModelState

/** Setup's selection only counts during setup; in manage mode a pack is checked only once installed. */
internal fun packChecked(core: Boolean, installed: Boolean, setup: Boolean, selected: Boolean): Boolean =
    core || installed || (setup && selected)

/**
 * First-run setup ([setup] = true: core models plus chosen packs, one Download button) and the
 * Languages screen (per-pack download and delete).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ModelsScreen(
    models: List<ModelInfo>,
    downloads: DownloadState,
    setup: Boolean,
    error: String?,
    onDownload: (List<String>) -> Unit,
    onCancel: () -> Unit,
    onDelete: (String) -> Unit,
    onBack: (() -> Unit)?,
) {
    val context = LocalContext.current
    var selected by rememberSaveable { mutableStateOf(setOf("rec-latin")) }
    val pending = models.filter { it.state != ModelState.INSTALLED }
    val wanted = if (setup) pending.filter { it.core || it.id in selected } else emptyList()
    val needed = wanted.sumOf { it.remainingBytes() }
    val hasPack = models.any { !it.core && (it.state == ModelState.INSTALLED || it.id in selected) }
    var free by remember { mutableLongStateOf(Long.MAX_VALUE) }
    LaunchedEffect(downloads.finished, models) {
        free = withContext(Dispatchers.IO) { StatFs(context.filesDir.path).availableBytes }
    }

    val row: @Composable (ModelInfo) -> Unit = { m ->
        ModelRow(
            model = m,
            progress = downloads.progress[m.id].takeIf { downloads.running && m.state != ModelState.INSTALLED },
            size = formatSize(m.sizeBytes.toLong()),
            onCheck = if (setup && !m.core && m.state != ModelState.INSTALLED) {
                { checked: Boolean -> selected = if (checked) selected + m.id else selected - m.id }
            } else {
                null
            },
            checked = packChecked(m.core, m.state == ModelState.INSTALLED, setup, m.id in selected),
            actions = when {
                setup || m.core -> null
                downloads.busy -> if (m.id in downloads.progress && m.state != ModelState.INSTALLED) {
                    { SmallOutlinedButton("Cancel", onCancel) }
                } else {
                    null
                }
                else -> {
                    {
                        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            if (m.state != ModelState.INSTALLED) {
                                SmallButton(
                                    if (m.state == ModelState.INCOMPLETE) "Resume" else "Download",
                                    enabled = m.remainingBytes() <= free,
                                ) { onDownload(listOf(m.id)) }
                            }
                            if (m.state != ModelState.NOT_INSTALLED || m.downloadedBytes > 0u) {
                                SmallOutlinedButton("Delete") { onDelete(m.id) }
                            }
                        }
                    }
                }
            },
        )
    }

    Scaffold(
        topBar = {
            if (onBack != null) {
                TopAppBar(
                    title = { Text("Languages") },
                    navigationIcon = {
                        IconButton(onClick = onBack) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                        }
                    },
                    colors = appBarColors(),
                )
            }
        },
        bottomBar = {
            Column(
                Modifier.navigationBarsPadding().padding(16.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                (error ?: downloads.error)?.let {
                    Text(it, color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(bottom = 8.dp))
                }
                if (downloads.waitingForUnmetered) {
                    Text("Waiting for Wi-Fi to download.", modifier = Modifier.padding(bottom = 8.dp))
                } else if (downloads.queued && !downloads.running && downloads.error == null) {
                    Text("Download queued.", modifier = Modifier.padding(bottom = 8.dp))
                }
                if (downloads.busy && setup) {
                    OutlinedButton(onClick = onCancel, modifier = Modifier.fillMaxWidth().height(56.dp)) {
                        Text("Cancel download")
                    }
                } else if (setup) {
                    if (needed > free) {
                        Text(
                            "Needs ${formatSize(needed)}, only ${formatSize(free)} free",
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.padding(bottom = 8.dp),
                        )
                    }
                    Button(
                        onClick = { onDownload(wanted.map { it.id }) },
                        enabled = wanted.isNotEmpty() && hasPack && needed <= free,
                        modifier = Modifier.fillMaxWidth().height(56.dp),
                    ) { Text(if (needed > 0) "Download ${formatSize(needed)}" else "Download") }
                    Text(
                        if (free == Long.MAX_VALUE) "Wi-Fi recommended" else "Wi-Fi recommended · ${formatSize(free)} free on this phone",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(top = 8.dp),
                    )
                }
            }
        },
    ) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()).padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            if (setup) Hero()
            SectionCard("Always included") { models.filter { it.core }.forEach { row(it) } }
            SectionCard("Scripts to read") { models.filter { !it.core }.forEach { row(it) } }
            Spacer(Modifier.height(8.dp))
        }
    }
}

@Composable
private fun Hero() {
    Column(
        Modifier.fillMaxWidth().padding(top = 24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Image(painterResource(R.drawable.logo_cup), contentDescription = null, modifier = Modifier.size(96.dp))
        Text("Welcome to translatte", style = DisplayTitleStyle, color = MaterialTheme.colorScheme.onBackground)
        Text(
            "Download the languages you read. After that, everything runs on your phone, with no internet needed.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun SectionCard(title: String, content: @Composable ColumnScope.() -> Unit) {
    Surface(shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.surface) {
        Column(Modifier.fillMaxWidth().padding(vertical = 8.dp)) {
            Text(
                title.uppercase(),
                style = MaterialTheme.typography.labelMedium.copy(letterSpacing = 1.sp),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(start = 16.dp, top = 4.dp, bottom = 4.dp).semantics { heading() },
            )
            content()
        }
    }
}

private fun ModelInfo.remainingBytes() = (sizeBytes - downloadedBytes).toLong()

@Composable
private fun ModelRow(
    model: ModelInfo,
    progress: Pair<Long, Long>?,
    size: String,
    onCheck: ((Boolean) -> Unit)?,
    checked: Boolean,
    actions: (@Composable () -> Unit)?,
) {
    Column(Modifier.padding(horizontal = 12.dp, vertical = 8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (onCheck != null) {
                Checkbox(checked = checked, onCheckedChange = onCheck)
            } else {
                Icon(
                    if (checked) Icons.Filled.CheckCircle else Icons.Outlined.Circle,
                    contentDescription = if (checked) "Installed" else "Not installed",
                    tint = if (checked) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline,
                    modifier = Modifier.padding(12.dp).size(24.dp),
                )
            }
            Spacer(Modifier.width(4.dp))
            Column(Modifier.weight(1f)) {
                Text(model.displayName, style = MaterialTheme.typography.titleMedium)
                if (model.languages.isNotEmpty()) {
                    Text(
                        model.languages.joinToString(", "),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                val status = when {
                    model.core -> "Required · $size"
                    model.state == ModelState.INSTALLED -> "Installed · $size"
                    model.state == ModelState.INCOMPLETE -> "Incomplete · $size"
                    else -> size
                }
                Text(status, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            actions?.invoke()
        }
        if (progress != null) {
            val (done, total) = progress
            LinearProgressIndicator(
                progress = { if (total > 0) done.toFloat() / total else 0f },
                modifier = Modifier.fillMaxWidth().padding(top = 4.dp),
            )
            Text(
                if (done == 0L) {
                    "Waiting for data…"
                } else {
                    "${formatSize(done)} of ${formatSize(total)}"
                },
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

private val SmallButtonPadding = PaddingValues(horizontal = 14.dp)

@Composable
private fun SmallButton(label: String, enabled: Boolean = true, onClick: () -> Unit) {
    Button(onClick, Modifier.height(34.dp), enabled = enabled, contentPadding = SmallButtonPadding) { Text(label) }
}

@Composable
private fun SmallOutlinedButton(label: String, onClick: () -> Unit) {
    OutlinedButton(onClick, Modifier.height(34.dp), contentPadding = SmallButtonPadding) { Text(label) }
}
