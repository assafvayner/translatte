import Foundation

enum LanguageDirection {
    /// "Japanese → English", falling back to the code for an unknown source.
    static func label(source: String, sources: [LanguageInfo], target: LanguageInfo?) -> String {
        let from = sources.first { $0.code == source }?.name ?? source
        return "\(from) → \(target?.name ?? "English")"
    }

    /// The saved `source` if it's still offered, else the first language in the (alphabetical)
    /// list: a choice can disappear when an app update drops a language or its pack is deleted.
    /// Same rule as Android's `fallbackSource`.
    static func fallback(source: String, sources: [LanguageInfo]) -> String {
        sources.contains { $0.code == source } ? source : sources.first?.code ?? ""
    }
}

extension TranslatorModel {
    var directionLabel: String {
        LanguageDirection.label(source: sourceCode, sources: sourceLanguages, target: target)
    }

    var targetIsRTL: Bool {
        target?.rtl ?? false
    }
}
