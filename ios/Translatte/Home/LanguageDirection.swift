import Foundation

enum LanguageDirection {
    /// "Japanese → English", falling back to codes for unknown languages.
    static func label(source: String, target: String, sources: [LanguageInfo], targets: [LanguageInfo]) -> String {
        let from = sources.first { $0.code == source }?.name ?? source
        let to = targets.first { $0.code == target }?.name ?? target
        return "\(from) → \(to)"
    }

    /// The saved `source` and `target` if both are still offered, else replacements: a choice can
    /// disappear when an app update drops a language or its script pack is deleted. A replacement is
    /// English if possible, else the first language in the (alphabetical) list, and never the same as
    /// the other side; the target changes if only English is left to read. Same rule as Android's
    /// `fallbackLanguages`.
    static func fallback(
        source: String, target: String, sources: [LanguageInfo], targets: [LanguageInfo]
    ) -> (source: String, target: String) {
        func pick(_ languages: [LanguageInfo], avoiding avoid: String?) -> String? {
            (languages.first { $0.code == "en" && $0.code != avoid } ?? languages.first { $0.code != avoid })?.code
        }
        let s = sources.contains { $0.code == source }
            ? source : pick(sources, avoiding: target) ?? pick(sources, avoiding: nil) ?? ""
        var t = targets.contains { $0.code == target } ? target : pick(targets, avoiding: s) ?? ""
        if s == t, s != source || t != target {
            t = pick(targets, avoiding: s) ?? t
        }
        return (s, t)
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
