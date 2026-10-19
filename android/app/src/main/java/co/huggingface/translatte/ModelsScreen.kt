package co.huggingface.translatte

import android.os.StatFs
import android.text.format.Formatter
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import uniffi.translatte_ffi.ModelInfo
import uniffi.translatte_ffi.ModelState

/**
 * First-run setup ([setup] = true: core models plus chosen packs, one Download button) and the
 * Manage languages screen (per-pack download and delete).
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
    val free = remember(downloads.finished, models) { StatFs(context.filesDir.path).availableBytes }
    fun size(bytes: Long) = Formatter.formatShortFileSize(context, bytes)

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(if (setup) "Set up Translatte" else "Languages") },
                navigationIcon = {
                    if (onBack != null) {
                        IconButton(onClick = onBack) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                        }
                    }
                },
            )
        },
        bottomBar = {
            Column(Modifier.padding(16.dp)) {
                (error ?: downloads.error)?.let {
                    Text(it, color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(bottom = 8.dp))
                }
                if (downloads.waitingForUnmetered) {
                    Text("Waiting for Wi-Fi to download.", modifier = Modifier.padding(bottom = 8.dp))
                }
                if (downloads.running || downloads.waitingForUnmetered) {
                    OutlinedButton(onClick = onCancel, modifier = Modifier.fillMaxWidth()) { Text("Cancel download") }
                } else if (setup) {
                    if (needed > free) {
                        Text(
                            "Needs ${size(needed)}, only ${size(free)} free",
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.padding(bottom = 8.dp),
                        )
                    }
                    Button(
                        onClick = { onDownload(wanted.map { it.id }) },
                        enabled = wanted.isNotEmpty() && hasPack && needed <= free,
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text(if (needed > 0) "Download (${size(needed)})" else "Download") }
                }
            }
        },
    ) { padding ->
        LazyColumn(
            Modifier.fillMaxSize().padding(padding),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (setup) {
                item {
                    Text(
                        "Translatte works offline once its models are on the phone. The text reader " +
                            "and translator are required; pick the scripts you want to read.",
                    )
                }
            }
            items(models, key = { it.id }) { m ->
                ModelRow(
                    model = m,
                    progress = downloads.progress[m.id].takeIf { downloads.running && m.state != ModelState.INSTALLED },
                    size = size(m.sizeBytes.toLong()),
                    checkbox = if (setup && !m.core && m.state != ModelState.INSTALLED) {
                        { checked -> selected = if (checked) selected + m.id else selected - m.id }
                    } else {
                        null
                    },
                    checked = m.core || m.id in selected || m.state == ModelState.INSTALLED,
                    actions = if (setup || m.core || downloads.running) {
                        null
                    } else {
                        {
                            Column(horizontalAlignment = Alignment.End) {
                                if (m.state != ModelState.INSTALLED) {
                                    Button(
                                        onClick = { onDownload(listOf(m.id)) },
                                        enabled = m.remainingBytes() <= free,
                                    ) { Text(if (m.state == ModelState.INCOMPLETE) "Resume" else "Download") }
                                }
                                if (m.state != ModelState.NOT_INSTALLED) {
                                    OutlinedButton(onClick = { onDelete(m.id) }) { Text("Delete") }
                                }
                            }
                        }
                    },
                )
            }
        }
    }
}

private fun ModelInfo.remainingBytes() = (sizeBytes - downloadedBytes).toLong()

@Composable
private fun ModelRow(
    model: ModelInfo,
    progress: Pair<Long, Long>?,
    size: String,
    checkbox: ((Boolean) -> Unit)?,
    checked: Boolean,
    actions: (@Composable () -> Unit)?,
) {
    Column {
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (checkbox != null) {
                Checkbox(checked = checked, onCheckedChange = checkbox)
            } else {
                Checkbox(checked = checked, onCheckedChange = null, enabled = false)
            }
            Spacer(Modifier.width(8.dp))
            Column(Modifier.weight(1f)) {
                Text(model.displayName, style = MaterialTheme.typography.titleMedium)
                val status = when {
                    model.core -> "Required · $size"
                    model.state == ModelState.INSTALLED -> "Installed · $size"
                    model.state == ModelState.INCOMPLETE -> "Incomplete · $size"
                    else -> size
                }
                Text(status, style = MaterialTheme.typography.bodySmall)
                if (model.languages.isNotEmpty()) {
                    Text(
                        model.languages.joinToString(", "),
                        style = MaterialTheme.typography.bodySmall,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            actions?.invoke()
        }
        if (progress != null) {
            val (done, total) = progress
            val context = LocalContext.current
            LinearProgressIndicator(
                progress = { if (total > 0) done.toFloat() / total else 0f },
                modifier = Modifier.fillMaxWidth().padding(top = 4.dp),
            )
            Text(
                if (done == 0L) {
                    "Waiting for data…"
                } else {
                    "${Formatter.formatShortFileSize(context, done)} of ${Formatter.formatShortFileSize(context, total)}"
                },
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}
