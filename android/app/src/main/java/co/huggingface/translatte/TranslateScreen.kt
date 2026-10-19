package co.huggingface.translatte

import android.content.ActivityNotFoundException
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Translate
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MenuAnchorType
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import uniffi.translatte_ffi.LanguageInfo
import uniffi.translatte_ffi.Region
import java.io.File

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TranslateScreen(vm: MainViewModel, onManageLanguages: () -> Unit) {
    val context = LocalContext.current
    val captureUri = remember {
        val file = File(context.cacheDir, "photos/capture.jpg").apply { parentFile?.mkdirs() }
        FileProvider.getUriForFile(context, "${context.packageName}.photos", file)
    }
    val camera = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { ok ->
        if (ok) vm.translate(captureUri)
    }
    val gallery = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri: Uri? ->
        uri?.let(vm::translate)
    }
    var showOriginal by rememberSaveable { mutableStateOf(false) }
    var tapped by remember { mutableStateOf<Region?>(null) }
    var noCamera by remember { mutableStateOf(false) }
    val working = vm.phase is Phase.Working

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Translatte") },
                actions = {
                    IconButton(onClick = onManageLanguages, enabled = !working) {
                        Icon(Icons.Filled.Translate, contentDescription = "Languages")
                    }
                },
            )
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding).padding(horizontal = 16.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                LanguagePicker("From", vm.sourceLanguages, vm.sourceCode, vm::setSource, Modifier.weight(1f))
                LanguagePicker("To", vm.targetLanguages, vm.targetCode, vm::setTarget, Modifier.weight(1f))
            }
            Box(Modifier.weight(1f).fillMaxWidth().padding(vertical = 8.dp), contentAlignment = Alignment.Center) {
                val image = vm.image
                if (image != null) {
                    OverlayImage(image, vm.result, showOriginal, onTap = { tapped = it }, modifier = Modifier.fillMaxSize())
                } else {
                    Text("Take a photo of a sign, menu or label.", style = MaterialTheme.typography.bodyLarge)
                }
            }
            vm.memoryWarning?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            if (noCamera) Text("No camera app found; choose a photo instead.", color = MaterialTheme.colorScheme.error)
            when (val p = vm.phase) {
                is Phase.Working -> {
                    Text(p.label)
                    LinearProgressIndicator(Modifier.fillMaxWidth().padding(vertical = 4.dp))
                }
                is Phase.Failed -> Text(p.message, color = MaterialTheme.colorScheme.error)
                else -> {}
            }
            if (vm.result != null) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Show original", Modifier.weight(1f))
                    Switch(checked = showOriginal, onCheckedChange = { showOriginal = it })
                }
            }
            Row(Modifier.padding(vertical = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    onClick = {
                        try {
                            camera.launch(captureUri)
                        } catch (e: ActivityNotFoundException) {
                            noCamera = true
                        }
                    },
                    enabled = !working,
                    modifier = Modifier.weight(1f),
                ) {
                    Text("Take photo")
                }
                OutlinedButton(
                    onClick = { gallery.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) },
                    enabled = !working,
                    modifier = Modifier.weight(1f),
                ) { Text("Choose photo") }
            }
        }
    }

    tapped?.let { region ->
        AlertDialog(
            onDismissRequest = { tapped = null },
            confirmButton = { TextButton(onClick = { tapped = null }) { Text("Close") } },
            title = { Text(region.translatedText ?: "") },
            text = { Text(region.sourceText) },
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun LanguagePicker(
    label: String,
    languages: List<LanguageInfo>,
    selected: String,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var expanded by remember { mutableStateOf(false) }
    ExposedDropdownMenuBox(expanded = expanded, onExpandedChange = { expanded = it }, modifier = modifier) {
        OutlinedTextField(
            value = languages.firstOrNull { it.code == selected }?.name ?: "",
            onValueChange = {},
            readOnly = true,
            singleLine = true,
            label = { Text(label) },
            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded) },
            modifier = Modifier.menuAnchor(MenuAnchorType.PrimaryNotEditable).fillMaxWidth(),
        )
        ExposedDropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            languages.forEach { l ->
                DropdownMenuItem(text = { Text(l.name) }, onClick = {
                    onSelect(l.code)
                    expanded = false
                })
            }
        }
    }
}
