package co.huggingface.translatte

import android.Manifest
import android.content.pm.ApplicationInfo
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import java.io.File

class MainActivity : ComponentActivity() {
    private val vm: MainViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val notifications = registerForActivityResult(ActivityResultContracts.RequestPermission()) {}

        setContent {
            MaterialTheme(colorScheme = if (isSystemInDarkTheme()) darkColorScheme() else lightColorScheme()) {
                val downloads by Downloads.state.collectAsStateWithLifecycle()
                var managing by rememberSaveable { mutableStateOf(false) }
                LaunchedEffect(downloads.finished) { vm.refresh() }

                val startDownload = { ids: List<String> ->
                    notifications.launch(Manifest.permission.POST_NOTIFICATIONS)
                    Downloads.start(this, ids)
                }
                when {
                    !vm.ready -> ModelsScreen(
                        models = vm.models,
                        downloads = downloads,
                        setup = true,
                        error = vm.setupError,
                        onDownload = startDownload,
                        onCancel = { Downloads.cancel(this) },
                        onDelete = vm::deleteModel,
                        onBack = null,
                    )
                    managing -> {
                        BackHandler { managing = false }
                        ModelsScreen(
                            models = vm.models,
                            downloads = downloads,
                            setup = false,
                            error = vm.setupError,
                            onDownload = startDownload,
                            onCancel = { Downloads.cancel(this) },
                            onDelete = vm::deleteModel,
                            onBack = { managing = false },
                        )
                    }
                    else -> TranslateScreen(vm, onManageLanguages = { managing = true })
                }
            }
        }
        if (savedInstanceState == null) runAutotest()
    }

    override fun onStart() {
        super.onStart()
        vm.prepare()
    }

    /**
     * Debug builds only: `am start ... --es autotest_download det,rec-latin` starts a download, and
     * `--es autotest_image <file in getExternalFilesDir> --es autotest_from fr --es autotest_to en`
     * translates a photo and writes autotest.json next to it.
     */
    private fun runAutotest() {
        if (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE == 0) return
        intent.getStringExtra("autotest_download")?.let { Downloads.start(this, it.split(',')) }
        val dir = getExternalFilesDir(null) ?: return
        intent.getStringExtra("autotest_image")?.let { name ->
            vm.autotest(
                File(dir, name),
                intent.getStringExtra("autotest_from"),
                intent.getStringExtra("autotest_to"),
                File(dir, "autotest.json"),
            )
        }
    }
}
