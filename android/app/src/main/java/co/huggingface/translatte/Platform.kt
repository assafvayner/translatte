package co.huggingface.translatte

import android.content.Context

/** JNI entry points in libtranslatte_ffi.so that live outside the UniFFI bindings. */
object Platform {
    init {
        System.loadLibrary("translatte_ffi")
    }

    /**
     * Sends Rust logs to logcat and lets rustls verify certificates through Android's trust
     * manager. Call once at startup, before any download.
     */
    @JvmStatic
    external fun init(context: Context)
}
