package co.huggingface.translatte

import android.app.Notification
import android.content.Context
import android.content.pm.ServiceInfo
import android.os.SystemClock
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.ForegroundInfo
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import uniffi.translatte_ffi.DownloadListener
import uniffi.translatte_ffi.FfiException

data class DownloadState(
    val running: Boolean = false,
    /** Model id to (bytes done, bytes total) for models in the current or last run. */
    val progress: Map<String, Pair<Long, Long>> = emptyMap(),
    val error: String? = null,
    /** Bumped when a model finishes or a run ends, so screens re-read install state. */
    val finished: Int = 0,
)

/** Model downloads, run by WorkManager with a foreground notification so they survive backgrounding. */
object Downloads {
    private const val WORK_NAME = "models"
    private val mutableState = MutableStateFlow(DownloadState())
    val state: StateFlow<DownloadState> = mutableState

    fun start(context: Context, ids: List<String>) {
        if (ids.isEmpty()) return
        mutableState.update { it.copy(error = null) }
        val request = OneTimeWorkRequestBuilder<DownloadWorker>()
            .setInputData(workDataOf(DownloadWorker.IDS to ids.toTypedArray()))
            .build()
        WorkManager.getInstance(context)
            .enqueueUniqueWork(WORK_NAME, ExistingWorkPolicy.APPEND_OR_REPLACE, request)
    }

    fun cancel(context: Context) {
        WorkManager.getInstance(context).cancelUniqueWork(WORK_NAME)
    }

    internal fun update(f: (DownloadState) -> DownloadState) = mutableState.update(f)
}

class DownloadWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    private val engine = TranslatteApp.instance.engine
    private var lastNotified = 0L

    override suspend fun doWork(): Result = coroutineScope {
        val ids = inputData.getStringArray(IDS)?.toList().orEmpty()
        val sizes = runCatching { engine.models() }.getOrDefault(emptyList()).associate { it.id to it.sizeBytes.toLong() }
        Downloads.update { s ->
            s.copy(running = true, error = null, progress = s.progress + ids.associateWith { 0L to (sizes[it] ?: 0L) })
        }
        setForeground(foregroundInfo("Starting…", 0, 0))
        val listener = object : DownloadListener {
            override fun onProgress(modelId: String, done: ULong, total: ULong) {
                Downloads.update {
                    val complete = total > 0u && done == total && it.progress[modelId]?.first != done.toLong()
                    it.copy(
                        progress = it.progress + (modelId to (done.toLong() to total.toLong())),
                        finished = if (complete) it.finished + 1 else it.finished,
                    )
                }
                val now = SystemClock.elapsedRealtime()
                if (now - lastNotified >= 1000) {
                    lastNotified = now
                    val pct = if (total > 0u) (done * 100u / total).toInt() else 0
                    setForegroundAsync(foregroundInfo(modelId, pct, 100))
                }
            }
        }
        // The FFI call blocks; on cancellation, signal it and let it unwind on its own thread.
        val job = async(Dispatchers.IO) { runCatching { engine.download(ids, listener) } }
        val outcome = try {
            job.await()
        } catch (e: CancellationException) {
            engine.cancelDownload()
            Downloads.update { it.copy(running = false, finished = it.finished + 1) }
            throw e
        }
        val error = outcome.exceptionOrNull()?.let {
            when (it) {
                is FfiException.Cancelled -> null
                is FfiException.Failed -> it.reason
                else -> it.toString()
            }
        }
        Downloads.update { it.copy(running = false, error = error, finished = it.finished + 1) }
        if (error == null) Result.success() else Result.failure()
    }

    private fun foregroundInfo(text: String, progress: Int, max: Int): ForegroundInfo {
        val notification = Notification.Builder(applicationContext, TranslatteApp.DOWNLOAD_CHANNEL)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentTitle("Downloading models")
            .setContentText(text)
            .setProgress(max, progress, max == 0)
            .setOngoing(true)
            .build()
        return ForegroundInfo(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
    }

    companion object {
        const val IDS = "ids"
        private const val NOTIFICATION_ID = 1
    }
}
