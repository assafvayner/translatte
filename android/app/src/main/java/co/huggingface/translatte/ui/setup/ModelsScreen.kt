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
import uniffi.translatte_ffi.LanguagePack
import uniffi.translatte_ffi.ModelInfo
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.translatte_ffi.ModelState

/** Setup's selection only counts during setup; in manage mode a pack is checked only once installed. */
internal fun packChecked(core: Boolean, installed: Boolean, setup: Boolean, selected: Boolean): Boolean =
    core || installed || (setup && selected)

/** The models among [ids] still to download, each once. */
internal fun missingModels(models: List<ModelInfo>, ids: List<String>): List<String> =
    ids.distinct().filter { id -> models.firstOrNull { it.id == id }?.state != ModelState.INSTALLED }

/** Combined (bytes done, bytes total) of the models among [ids] in the current download, if any. */
internal fun packProgress(progress: Map<String, Pair<Long, Long>>, ids: List<String>): Pair<Long, Long>? {
    val parts = ids.mapNotNull { progress[it] }
    return if (parts.isEmpty()) null else parts.sumOf { it.first } to parts.sumOf { it.second }
}

/**
 * First-run setup ([setup] = true: the core models plus chosen languages, one Download button) and
 * the Languages screen (per-language download and delete). A language pack is its OCR script pack
 * plus its translation model; languages that share a script pack download it once.
 */
@Composable
fun ModelsScreen(
    models: List<ModelInfo>,
    packs: List<LanguagePack>,
    downloads: DownloadState,
    setup: Boolean,
    error: String?,
    onDownload: (List<String>) -> Unit,
    onCancel: () -> Unit,
    onDelete: (String) -> Unit,
    onBack: (() -> Unit)?,
) {
    val context = LocalContext.current
    var selected by rememberSaveable { mutableStateOf(setOf<String>()) }
    val core = models.filter { it.core }
    val wanted = if (setup) {
        missingModels(models, core.map { it.id } + packs.filter { it.code in selected }.flatMap { it.modelIds })
    } else {
        emptyList()
    }
    val needed = models.filter { it.id in wanted }.sumOf { it.remainingBytes() }
    val hasPack = packs.any { it.state == ModelState.INSTALLED || it.code in selected }
    var free by remember { mutableLongStateOf(Long.MAX_VALUE) }
    LaunchedEffect(downloads.finished, models) {
        free = withContext(Dispatchers.IO) { StatFs(context.filesDir.path).availableBytes }
    }
    val progressOf = { ids: List<String>, installed: Boolean ->
        if (downloads.running && !installed) packProgress(downloads.progress, ids) else null
    }

    val packRow: @Composable (LanguagePack) -> Unit = { p ->
        val installed = p.state == ModelState.INSTALLED
        val size = formatSize(p.sizeBytes.toLong())
        PackRow(
            title = p.name,
            status = when (p.state) {
                ModelState.INSTALLED -> "Installed · $size"
                ModelState.INCOMPLETE -> "Incomplete · $size"
                ModelState.NOT_INSTALLED -> size
            },
            progress = progressOf(p.modelIds, installed),
            onCheck = if (setup && !installed) {
                { checked: Boolean -> selected = if (checked) selected + p.code else selected - p.code }
            } else {
                null
            },
            checked = packChecked(false, installed, setup, p.code in selected),
            actions = when {
                setup -> null
                downloads.busy -> if (!installed && p.modelIds.any { it in downloads.progress }) {
                    { SmallOutlinedButton("Cancel", onClick = onCancel) }
                } else {
                    null
                }
                else -> {
                    {
                        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            if (!installed) {
                                SmallButton(
                                    if (p.state == ModelState.INCOMPLETE) "Resume" else "Download",
                                    enabled = p.remainingBytes() <= free,
                                ) { onDownload(missingModels(models, p.modelIds)) }
                            }
                            if (p.deletable) {
                                SmallOutlinedButton("Delete") { onDelete(p.code) }
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
                        onClick = { onDownload(wanted) },
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
            SectionCard("Always included") {
                core.forEach { m ->
                    PackRow(
                        title = m.displayName,
                        status = "Required · ${formatSize(m.sizeBytes.toLong())}",
                        progress = progressOf(listOf(m.id), m.state == ModelState.INSTALLED),
                        onCheck = null,
                        checked = true,
                        actions = null,
                    )
                }
            }
            SectionCard("Languages to read") { packs.forEach { packRow(it) } }
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

private fun LanguagePack.remainingBytes() = (sizeBytes - downloadedBytes).toLong()

@Composable
private fun PackRow(
    title: String,
    status: String,
    progress: Pair<Long, Long>?,
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
                    Text(title, style = AppType.headline)
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
