package co.huggingface.translatte.ui.settings

import uniffi.translatte_ffi.OcrEngine

/** Debug builds: which runtime runs OCR, for comparing them on one install. */
val OcrEngine.key: String
    get() = when (this) {
        OcrEngine.ORT -> "ort"
        OcrEngine.NCNN_FP32 -> "ncnn32"
        OcrEngine.NCNN_FP16 -> "ncnn16"
    }

val OcrEngine.label: String
    get() = when (this) {
        OcrEngine.ORT -> "ONNX Runtime"
        OcrEngine.NCNN_FP32 -> "NCNN fp32"
        OcrEngine.NCNN_FP16 -> "NCNN fp16"
    }

fun ocrEngineFromKey(key: String?): OcrEngine = OcrEngine.entries.firstOrNull { it.key == key } ?: OcrEngine.ORT
