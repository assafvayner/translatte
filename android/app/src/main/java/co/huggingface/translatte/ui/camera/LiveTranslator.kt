package co.huggingface.translatte.ui.camera

/** One camera frame as the engine takes it: tightly packed RGBA rows, before rotating upright. */
class LiveFrame(val rgba: ByteArray, val width: Int, val height: Int, val rotation: Int)
