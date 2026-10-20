import Foundation

enum LanguageDirection {
    /// "Japanese → English", falling back to codes for unknown languages.
    static func label(source: String, target: String, sources: [LanguageInfo], targets: [LanguageInfo]) -> String {
        let from = sources.first { $0.code == source }?.name ?? source
        let to = targets.first { $0.code == target }?.name ?? target
        return "\(from) → \(to)"
    }

    /// `code` if `languages` offers it, else English, else the first one ("" if none): a saved choice can
    /// disappear when an app update drops a language or its script pack is deleted.
    static func availableOrDefault(_ code: String, in languages: [LanguageInfo]) -> String {
        if languages.contains(where: { $0.code == code }) { return code }
        return (languages.first { $0.code == "en" } ?? languages.first)?.code ?? ""
    }

    /// Swapping makes the target the source, so the target must be readable with the installed packs.
    static func canSwap(source: String, target: String, sources: [LanguageInfo]) -> Bool {
        source != target && sources.contains { $0.code == target }
    }
}

extension TranslatorModel {
    var directionLabel: String {
        LanguageDirection.label(source: sourceCode, target: targetCode, sources: sourceLanguages, targets: targetLanguages)
    }

    var canSwapLanguages: Bool {
        LanguageDirection.canSwap(source: sourceCode, target: targetCode, sources: sourceLanguages)
    }

    var targetIsRTL: Bool {
        targetLanguages.first { $0.code == targetCode }?.rtl ?? false
    }
}
