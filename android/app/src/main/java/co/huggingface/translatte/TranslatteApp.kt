package co.huggingface.translatte

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.ComponentCallbacks2
import android.content.Context
import android.content.pm.ApplicationInfo
import android.system.Os
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.translatte_ffi.Engine
import uniffi.translatte_ffi.configureDownloads
import java.io.File
import kotlin.time.Duration.Companion.minutes

class TranslatteApp : Application() {
    /** Created on first use, by whichever of the UI or a download worker gets there first. */
    val engine: Engine by lazy {
        val modelsDir = File(filesDir, "models").apply { mkdirs() }
        val manifest = assets.open("manifest.json").bufferedReader().use { it.readText() }
        Engine(modelsDir.path, manifest)
    }

    /** Serializes calls that load, use or free models, so the engine never runs two at once. */
    @OptIn(ExperimentalCoroutinesApi::class)
    val engineDispatcher = Dispatchers.IO.limitedParallelism(1)
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var releaseJob: Job? = null

    /** Whether the engine holds loaded models; only touched on [engineDispatcher]. */
    var modelsLoaded = false

    /**
     * Runs before content providers (and so before WorkManager starts its threads): the native
     * setup sets process environment variables, which isn't safe once other threads read them.
     */
    override fun attachBaseContext(base: Context) {
        super.attachBaseContext(base)
        // Debug builds: `run-as <pkg> sh -c 'echo xet_data=debug > files/rust_log'` sets RUST_LOG.
        if (base.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0) {
            File(base.filesDir, "rust_log").takeIf { it.exists() }?.let { Os.setenv("RUST_LOG", it.readText().trim(), true) }
        }
        configureDownloads(File(base.cacheDir, "hf").path)
        Platform.init(base)
    }

    override fun onCreate() {
        super.onCreate()
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(DOWNLOAD_CHANNEL, "Model downloads", NotificationManager.IMPORTANCE_LOW),
        )

        // Free the ~3GB of loaded models once the app has been in the background for a while.
        ProcessLifecycleOwner.get().lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onStart(owner: LifecycleOwner) {
                releaseJob?.cancel()
            }

            override fun onStop(owner: LifecycleOwner) {
                releaseJob = scope.launch {
                    delay(RELEASE_AFTER)
                    releaseModels()
                }
            }
        })
    }

    /**
     * A backgrounded app holding ~3GB is the low-memory killer's first pick (e.g. while the camera
     * app is open), so free the models as soon as the system asks instead of waiting for the timer.
     * They're prepared again when the app returns to the foreground.
     */
    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        val background = !ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)
        if (background && level >= ComponentCallbacks2.TRIM_MEMORY_BACKGROUND) releaseModels()
    }

    private fun releaseModels() {
        scope.launch(engineDispatcher) {
            if (modelsLoaded) {
                engine.unload()
                modelsLoaded = false
            }
        }
    }

    companion object {
        const val DOWNLOAD_CHANNEL = "downloads"
        private val RELEASE_AFTER = 2.minutes
    }
}
