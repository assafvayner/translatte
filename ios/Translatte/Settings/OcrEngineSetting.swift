import Foundation

/// Debug builds: which runtime runs OCR, for comparing them on one install; persisted under `storageKey`.
enum OcrEngineSetting: String, CaseIterable, Identifiable {
    case ort
    case ncnn32
    case ncnn16

    static let storageKey = "ocrEngine"

    static var stored: OcrEngineSetting {
        UserDefaults.standard.string(forKey: storageKey).flatMap(OcrEngineSetting.init(rawValue:)) ?? .ort
    }

    var id: String { rawValue }

    var title: String {
        switch self {
        case .ort: "ONNX Runtime"
        case .ncnn32: "NCNN fp32"
        case .ncnn16: "NCNN fp16"
        }
    }

    var engine: OcrEngine {
        switch self {
        case .ort: .ort
        case .ncnn32: .ncnnFp32
        case .ncnn16: .ncnnFp16
        }
    }
}
