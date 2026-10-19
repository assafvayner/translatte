package co.huggingface.translatte

import android.app.Notification
import android.content.Context
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.os.SystemClock
import android.util.Log
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.ForegroundInfo
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.WorkManager
import androidx.work.WorkRequest
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withContext
import uniffi.translatte_ffi.DownloadListener
import uniffi.translatte_ffi.DownloadToken
import uniffi.translatte_ffi.FfiException
import java.util.concurrent.TimeUnit

data class DownloadState(
    /** Download calls in flight; a replaced one can still be unwinding when the next starts. */
    val active: Int = 0,
    /** Model id to (bytes done, bytes total) for models in the current or last run. */
    val progress: Map<String, Pair<Long, Long>> = emptyMap(),
    val error: String? = null,
    /** Queued until an unmetered network is available. */
    val waitingForUnmetered: Boolean = false,
    /** Bumped when a model finishes or a run ends, so screens re-read install state. */
    val finished: Int = 0,
) {
    val running get() = active > 0
}

/** Model downloads, run by WorkManager with a foreground notification so they survive backgrounding. */
object Downloads {
    private const val WORK_NAME = "models"

    /** Downloads at least this big ask before using a metered connection. */
    const val LARGE_DOWNLOAD = 100_000_000L

    private val mutableState = MutableStateFlow(DownloadState())
    val state: StateFlow<DownloadState> = mutableState

    fun isMetered(context: Context): Boolean =
        context.getSystemService(ConnectivityManager::class.java).isActiveNetworkMetered

    /** Queues [ids]; with [allowMetered] false the download waits for an unmetered network. */
    fun start(context: Context, ids: List<String>, allowMetered: Boolean) {
        if (ids.isEmpty()) return
        val network = if (allowMetered) NetworkType.CONNECTED else NetworkType.UNMETERED
        mutableState.update { it.copy(error = null, waitingForUnmetered = !allowMetered && isMetered(context)) }
        val request = OneTimeWorkRequestBuilder<DownloadWorker>()
            .setInputData(workDataOf(DownloadWorker.IDS to ids.toTypedArray()))
            .setConstraints(Constraints.Builder().setRequiredNetworkType(network).build())
            .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
            .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, WorkRequest.MIN_BACKOFF_MILLIS, TimeUnit.MILLISECONDS)
            .build()
        WorkManager.getInstance(context)
            .enqueueUniqueWork(WORK_NAME, ExistingWorkPolicy.APPEND_OR_REPLACE, request)
    }

    fun cancel(context: Context) {
        WorkManager.getInstance(context).cancelUniqueWork(WORK_NAME)
        mutableState.update { it.copy(waitingForUnmetered = false) }
    }

    internal fun update(f: (DownloadState) -> DownloadState) = mutableState.update(f)
}

class DownloadWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    private var lastNotified = 0L

    override suspend fun getForegroundInfo() = foregroundInfo("Starting…", 0, 0)

    override suspend fun doWork(): Result {
        try {
            setForeground(getForegroundInfo())
        } catch (e: IllegalStateException) {
            // ForegroundServiceStartNotAllowedException: WorkManager re-ran this from the background
            // (after process death or a system stop). Try again later, e.g. once the app is opened.
            Log.w(TAG, "can't start the download in the foreground yet: $e")
            return Result.retry()
        }
        val engine = (applicationContext as TranslatteApp).engine
        val ids = inputData.getStringArray(IDS)?.toList().orEmpty()
        val sizes = runCatching { engine.models() }.getOrDefault(emptyList()).associate { it.id to it.sizeBytes.toLong() }
        Downloads.update { s ->
            s.copy(
                active = s.active + 1,
                error = null,
                waitingForUnmetered = false,
                progress = s.progress + ids.associateWith { 0L to (sizes[it] ?: 0L) },
            )
        }
        var error: String? = null
        try {
            error = download(engine, ids)
        } finally {
            Downloads.update { it.copy(active = it.active - 1, finished = it.finished + 1) }
        }
        return when {
            error == null -> Result.success()
            runAttemptCount + 1 < MAX_ATTEMPTS -> {
                Downloads.update { it.copy(error = "Download interrupted, retrying: $error") }
                Result.retry()
            }
            else -> {
                // Work appended behind this one is cancelled with it; drop its progress too.
                Downloads.update { it.copy(error = error, progress = emptyMap()) }
                Result.failure()
            }
        }
    }

    /** Runs the blocking download; returns the error to show, or null on success or cancel. */
    private suspend fun download(engine: uniffi.translatte_ffi.Engine, ids: List<String>): String? = coroutineScope {
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
        val token = DownloadToken()
        val job = async(Dispatchers.IO) { runCatching { engine.download(ids, listener, token) } }
        val outcome = try {
            job.await()
        } catch (e: CancellationException) {
            // The FFI call blocks: signal it, and only report the download stopped once it has.
            token.cancel()
            withContext(NonCancellable) { job.join() }
            throw e
        } finally {
            token.destroy()
        }
        when (val e = outcome.exceptionOrNull()) {
            null, is FfiException.Cancelled -> null
            is FfiException.Failed -> e.reason
            else -> e.toString()
        }
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
        private const val MAX_ATTEMPTS = 5
        private const val TAG = "translatte"
    }
}
