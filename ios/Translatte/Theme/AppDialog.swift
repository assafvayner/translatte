import SwiftUI

/// One dialog look for every prompt, matching Android's AlertDialog: sheet color, 28pt corners,
/// title and body tokens, right-aligned text buttons. System alerts would ignore the app's font.
struct AppDialog: Identifiable {
    struct Action {
        let title: String
        var id: String?
        var run: () -> Void = {}
    }

    let id = UUID()
    let title: String
    let message: String
    /// Long text (a license) scrolls in the caption token.
    var scrolls = false
    /// Left to right; the last is the confirming one.
    let actions: [Action]
    var identifier: String?
}

@MainActor
final class DialogCenter: ObservableObject {
    @Published private(set) var current: AppDialog?

    func show(_ dialog: AppDialog) {
        withAnimation(.easeOut(duration: 0.15)) { current = dialog }
    }

    func dismiss() {
        withAnimation(.easeIn(duration: 0.1)) { current = nil }
    }
}

/// Covers the whole window (navigation bars included) with the current dialog, if any. It sits under
/// any presented sheet, so dialogs must not be shown from inside a sheet.
struct DialogHost: View {
    @EnvironmentObject private var center: DialogCenter

    var body: some View {
        ZStack {
            if let dialog = center.current {
                Color.black.opacity(0.32)
                    .ignoresSafeArea()
                    .onTapGesture(perform: center.dismiss)
                    .accessibilityHidden(true)
                    .transition(.opacity)
                DialogCard(dialog: dialog, close: center.dismiss)
                    .padding(.horizontal, 24)
                    .transition(.opacity.combined(with: .scale(scale: 0.96)))
            }
        }
    }
}

private struct DialogCard: View {
    let dialog: AppDialog
    let close: () -> Void
    @AccessibilityFocusState private var titleFocused: Bool
    @FocusState private var keyboardFocused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(dialog.title)
                .font(Typography.title)
                .foregroundStyle(Palette.text)
                .accessibilityAddTraits(.isHeader)
                .accessibilityFocused($titleFocused)
            if dialog.scrolls {
                ScrollView {
                    message
                        .font(Typography.caption)
                        .lineSpacing(Typography.captionLineSpacing)
                }
                .frame(maxHeight: 420)
            } else {
                message
                    .font(Typography.body)
                    .lineSpacing(Typography.bodyLineSpacing)
            }
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 8) {
                    Spacer(minLength: 0)
                    buttons
                }
                VStack(alignment: .trailing, spacing: 0) {
                    buttons
                }
                .frame(maxWidth: .infinity, alignment: .trailing)
            }
            .padding(.top, 8)
        }
        .padding(24)
        .frame(maxWidth: 400, alignment: .leading)
        .background(Palette.sheet, in: RoundedRectangle(cornerRadius: Metrics.dialogRadius, style: .continuous))
        .accessibilityElement(children: .contain)
        .accessibilityAddTraits(.isModal)
        .accessibilityIdentifier(dialog.identifier ?? "dialog")
        .accessibilityAction(.escape, close)
        .focusable()
        .focusEffectDisabled()
        .focused($keyboardFocused)
        .onKeyPress(.escape) {
            close()
            return .handled
        }
        .onAppear {
            keyboardFocused = true
            UIAccessibility.post(notification: .screenChanged, argument: nil)
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { titleFocused = true }
        }
    }

    private var message: some View {
        Text(dialog.message)
            .foregroundStyle(Palette.textMuted)
            .frame(maxWidth: .infinity, alignment: .leading)
            .textSelection(.enabled)
    }

    @ViewBuilder private var buttons: some View {
        ForEach(Array(dialog.actions.enumerated()), id: \.offset) { _, action in
            Button(action.title) {
                close()
                action.run()
            }
            .buttonStyle(TextButtonStyle())
            .fixedSize()
            .accessibilityIdentifier(action.id ?? action.title)
        }
    }
}
