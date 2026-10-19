package co.huggingface.translatte

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.pm.ApplicationInfo
import android.system.Os
import androidx.lifecycle.DefaultLifecycleObserver
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
import uniffi.translatte_ffi.setCacheDir
import java.io.File
import kotlin.time.Duration.Companion.minutes

class TranslatteApp : Application() {
    lateinit var engine: Engine
        private set
    lateinit var modelsDir: File
        private set

    /** Serializes calls that load, use or free models, so the engine never runs two at once. */
    @OptIn(ExperimentalCoroutinesApi::class)
    val engineDispatcher = Dispatchers.IO.limitedParallelism(1)
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var releaseJob: Job? = null

    /** Whether the engine holds loaded models; only touched on [engineDispatcher]. */
    var modelsLoaded = false

    override fun onCreate() {
        super.onCreate()
        instance = this
        setCacheDir(File(cacheDir, "hf").path)
        // Debug builds: `run-as <pkg> sh -c 'echo xet_data=debug > files/rust_log'` sets RUST_LOG.
        if (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0) {
            File(filesDir, "rust_log").takeIf { it.exists() }?.let { Os.setenv("RUST_LOG", it.readText().trim(), true) }
        }
        Platform.init(this)
        modelsDir = File(filesDir, "models").apply { mkdirs() }
        val manifest = assets.open("manifest.json").bufferedReader().use { it.readText() }
        engine = Engine(modelsDir.path, manifest)

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
                    launch(engineDispatcher) {
                        engine.unload()
                        modelsLoaded = false
                    }
                }
            }
        })
    }

    companion object {
        const val DOWNLOAD_CHANNEL = "downloads"
        private val RELEASE_AFTER = 2.minutes
        lateinit var instance: TranslatteApp
            private set
    }
}
