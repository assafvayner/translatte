package co.huggingface.translatte.ui.setup

import android.os.StatFs
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.outlined.Circle
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
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
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import co.huggingface.translatte.DownloadState
import co.huggingface.translatte.R
import co.huggingface.translatte.formatSize
import co.huggingface.translatte.ui.common.AppTopBar
import co.huggingface.translatte.ui.common.NoticeBanner
import co.huggingface.translatte.ui.common.SectionCard
import co.huggingface.translatte.ui.common.SmallButton
import co.huggingface.translatte.ui.common.SmallOutlinedButton
import co.huggingface.translatte.ui.theme.AppType
import co.huggingface.translatte.ui.theme.Dimens
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
                    { SmallOutlinedButton("Cancel", onClick = onCancel) }
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
        topBar = { if (onBack != null) AppTopBar("Languages", onBack) },
        bottomBar = {
            Column(
                Modifier.navigationBarsPadding().padding(Dimens.gutter),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                (error ?: downloads.error)?.let { NoticeBanner(it) }
                val muted = MaterialTheme.colorScheme.onSurfaceVariant
                if (downloads.waitingForUnmetered) {
                    Text("Waiting for Wi-Fi to download.", color = muted, textAlign = TextAlign.Center)
                } else if (downloads.queued && !downloads.running && downloads.error == null) {
                    Text("Download queued.", color = muted, textAlign = TextAlign.Center)
                }
                if (downloads.busy && setup) {
                    OutlinedButton(
                        onClick = onCancel,
                        modifier = Modifier.fillMaxWidth().height(Dimens.buttonHeight),
                        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outline),
                    ) {
                        Text("Cancel download", color = MaterialTheme.colorScheme.onBackground)
                    }
                } else if (setup) {
                    if (needed > free) {
                        Text(
                            "Needs ${formatSize(needed)}, only ${formatSize(free)} free",
                            color = MaterialTheme.colorScheme.error,
                            textAlign = TextAlign.Center,
                        )
                    }
                    Button(
                        onClick = { onDownload(wanted.map { it.id }) },
                        enabled = wanted.isNotEmpty() && hasPack && needed <= free,
                        modifier = Modifier.fillMaxWidth().height(Dimens.buttonHeight),
                    ) { Text(if (needed > 0) "Download ${formatSize(needed)}" else "Download") }
                    if (free != Long.MAX_VALUE) {
                        Text(
                            "Wi-Fi recommended · ${formatSize(free)} free on this phone",
                            style = AppType.caption,
                            color = muted,
                        )
                    }
                }
            }
        },
    ) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()).padding(Dimens.gutter),
            verticalArrangement = Arrangement.spacedBy(Dimens.spacing),
        ) {
            if (setup) Hero()
            SectionCard("Always included") { models.filter { it.core }.forEach { row(it) } }
            SectionCard("Scripts to read") { models.filter { !it.core }.forEach { row(it) } }
        }
    }
}

@Composable
private fun Hero() {
    Column(
        Modifier.fillMaxWidth().padding(top = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Image(painterResource(R.drawable.logo_cup), contentDescription = null, modifier = Modifier.size(96.dp))
        Text(
            "Welcome to translatte",
            style = AppType.display,
            color = MaterialTheme.colorScheme.onBackground,
            modifier = Modifier.semantics { heading() },
        )
        Text(
            "Download the languages you read. After that, everything runs on your phone, with no internet needed.",
            style = AppType.body,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
            modifier = Modifier.widthIn(max = 300.dp),
        )
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
    val toggle = if (onCheck != null) {
        Modifier.toggleable(value = checked, role = Role.Checkbox, onValueChange = onCheck)
    } else {
        Modifier.semantics(mergeDescendants = true) {}
    }
    Column(Modifier.heightIn(min = Dimens.rowMinHeight).padding(horizontal = 16.dp, vertical = 10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(
                toggle.weight(1f),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Icon(
                    if (checked) Icons.Filled.CheckCircle else Icons.Outlined.Circle,
                    contentDescription = if (onCheck != null) null else if (checked) "Installed" else "Not installed",
                    tint = if (checked || onCheck != null) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline,
                    modifier = Modifier.size(24.dp),
                )
                Column(Modifier.weight(1f)) {
                    Text(model.displayName, style = AppType.headline)
                    if (model.languages.isNotEmpty()) {
                        Text(
                            model.languages.joinToString(", "),
                            style = AppType.caption,
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
                    Text(status, style = AppType.caption, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            actions?.invoke()
        }
        if (progress != null) {
            val (done, total) = progress
            LinearProgressIndicator(
                progress = { if (total > 0) done.toFloat() / total else 0f },
                modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
            )
            Text(
                if (done == 0L) {
                    "Waiting for data…"
                } else {
                    "${formatSize(done)} of ${formatSize(total)}"
                },
                style = AppType.caption,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(top = 4.dp),
            )
        }
    }
}
