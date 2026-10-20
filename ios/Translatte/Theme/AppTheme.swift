import SwiftUI
import UIKit

/// The user's theme choice, persisted per device under `storageKey`.
enum AppTheme: String, CaseIterable, Identifiable {
    case system, translatte, americano

    static let storageKey = "theme"

    var id: String { rawValue }

    var title: String { self == .system ? "System" : rawValue }

    /// The same copy as Android's theme rows.
    var description: String {
        switch self {
        case .system: "translatte in light mode, americano in dark mode"
        case .translatte: "Light"
        case .americano: "Dark"
        }
    }

    var colorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .translatte: .light
        case .americano: .dark
        }
    }

    var interfaceStyle: UIUserInterfaceStyle {
        switch self {
        case .system: .unspecified
        case .translatte: .light
        case .americano: .dark
        }
    }

    /// `preferredColorScheme(nil)` doesn't reliably restore the system appearance after a forced one;
    /// the window override does, and also covers sheets and alerts.
    @MainActor
    func applyToWindows() {
        for case let scene as UIWindowScene in UIApplication.shared.connectedScenes {
            scene.windows.forEach { $0.overrideUserInterfaceStyle = interfaceStyle }
        }
    }
}
