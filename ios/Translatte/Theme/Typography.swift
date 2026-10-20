import SwiftUI

enum Typography {
    static let frauncesName = "Fraunces72pt-SemiBold"
}

extension Font {
    /// Fraunces SemiBold, used only for the wordmark and display titles.
    static func display(_ size: CGFloat, relativeTo style: Font.TextStyle = .title) -> Font {
        .custom(Typography.frauncesName, size: size, relativeTo: style)
    }
}
