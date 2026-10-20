package co.huggingface.translatte

import android.Manifest
import android.app.UiModeManager
import android.content.pm.ApplicationInfo
import android.content.res.Configuration
import android.graphics.drawable.ColorDrawable
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.toArgb
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import co.huggingface.translatte.ui.MainNav
import co.huggingface.translatte.ui.Screen
import co.huggingface.translatte.ui.common.AppDialog
import co.huggingface.translatte.ui.settings.AppTheme
import co.huggingface.translatte.ui.settings.UiPrefs
import co.huggingface.translatte.ui.setup.ModelsScreen
import co.huggingface.translatte.ui.theme.windowBackground
import java.io.File

class MainActivity : ComponentActivity() {
    private val vm: MainViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val prefs = UiPrefs(this)
        val uiModes = getSystemService(UiModeManager::class.java)
        uiModes.setApplicationNightMode(prefs.theme.nightMode)
        val systemDark =
            resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES
        window.setBackgroundDrawable(ColorDrawable(windowBackground(prefs.theme.isDark(systemDark)).toArgb()))
        enableEdgeToEdge()
        val notifications = registerForActivityResult(ActivityResultContracts.RequestPermission()) {}

        setContent {
            var screen by remember { mutableStateOf<Screen?>(null) }
            AppTheme(prefs, applyNightMode = { uiModes.setApplicationNightMode(it.nightMode) }) { theme, dark, onTheme ->
                val lightBarIcons = dark || (vm.ready && screen == Screen.Camera)
                LaunchedEffect(lightBarIcons) {
                    val style = if (lightBarIcons) {
                        SystemBarStyle.dark(android.graphics.Color.TRANSPARENT)
                    } else {
                        SystemBarStyle.light(android.graphics.Color.TRANSPARENT, android.graphics.Color.TRANSPARENT)
                    }
                    enableEdgeToEdge(statusBarStyle = style, navigationBarStyle = style)
                }
                LaunchedEffect(dark) { window.setBackgroundDrawable(ColorDrawable(windowBackground(dark).toArgb())) }
                val downloads by remember { Downloads.state(applicationContext) }.collectAsStateWithLifecycle(DownloadState())
                LaunchedEffect(downloads.finished) { vm.refresh() }

                var meteredPrompt by rememberSaveable { mutableStateOf<List<String>?>(null) }
                val download = { ids: List<String>, allowMetered: Boolean ->
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                        notifications.launch(Manifest.permission.POST_NOTIFICATIONS)
                    }
                    Downloads.start(this, ids, allowMetered)
                }
                val startDownload = { ids: List<String> ->
                    if (Downloads.isMetered(this) && vm.bytesToDownload(ids) >= Downloads.LARGE_DOWNLOAD) {
                        meteredPrompt = ids
                    } else {
                        download(ids, true)
                    }
                }
                meteredPrompt?.let { ids ->
                    AppDialog(
                        title = "Download on mobile data?",
                        message = "This downloads ${formatSize(vm.bytesToDownload(ids))} over a metered connection.",
                        onDismiss = { meteredPrompt = null },
                        confirm = "Download now" to {
                            meteredPrompt = null
                            download(ids, true)
                        },
                        dismiss = listOf(
                            "Cancel" to { meteredPrompt = null },
                            "Wait for Wi-Fi" to {
                                meteredPrompt = null
                                download(ids, false)
                            },
                        ),
                    )
                }
                val modelsScreen = @Composable { setup: Boolean, onBack: (() -> Unit)? ->
                    ModelsScreen(
                        models = vm.models,
                        packs = vm.packs,
                        downloads = downloads,
                        setup = setup,
                        error = vm.setupError,
                        onDownload = startDownload,
                        onCancel = { Downloads.cancel(this) },
                        onDelete = vm::deleteLanguage,
                        onBack = onBack,
                    )
                }
                if (!vm.ready) {
                    modelsScreen(true, null)
                } else {
                    MainNav(
                        vm = vm,
                        prefs = prefs,
                        theme = theme,
                        onTheme = onTheme,
                        onScreenChange = { screen = it },
                        languagesScreen = { back -> modelsScreen(false, back) },
                    )
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
     * Debug builds only: `am start ... --es autotest_download det,rec-latin,pair-fr-en` starts a
     * download, and `--es autotest_image <file in getExternalFilesDir> --es autotest_from fr`
     * translates a photo and writes autotest.json next to it.
     */
    private fun runAutotest() {
        if (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE == 0) return
        intent.getStringExtra("autotest_download")?.let { Downloads.start(this, it.split(','), allowMetered = true) }
        val dir = getExternalFilesDir(null) ?: return
        intent.getStringExtra("autotest_image")?.let { name ->
            vm.autotest(
                File(dir, name),
                intent.getStringExtra("autotest_from"),
                File(dir, "autotest.json"),
            )
        }
    }
}
