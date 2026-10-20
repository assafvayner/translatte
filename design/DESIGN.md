# translatte design language

One look on Android (Jetpack Compose, Material 3) and iOS (SwiftUI), each built from the platform's
own primitives. Sizes are dp on Android and pt on iOS.

## Type

Fraunces everywhere (SIL Open Font License 1.1, `design/fonts/`), in two static cuts: 72pt for large
sizes and 9pt for text. Static Fraunces has no Medium, so only Regular and SemiBold are used. The
lowercase "translatte" wordmark stays as is.

| token | face | size | other | used for |
|---|---|---|---|---|
| wordmark | Fraunces 72pt SemiBold | 26 | tracking -0.3 | "translatte" in the Home header |
| display | Fraunces 72pt SemiBold | 24 | | "Welcome to translatte", empty-state title |
| title | Fraunces 72pt SemiBold | 20 | | top/nav bar titles, dialog titles, sheet titles |
| headline | Fraunces 9pt SemiBold | 17 | | card titles, "Last translation", list-row names, language names in the language bar |
| body | Fraunces 9pt Regular | 16 | line height ~22 | paragraphs, dialog text, list items |
| button | Fraunces 9pt SemiBold | 16 | | full-size and text buttons |
| label | Fraunces 9pt SemiBold | 14 | | small buttons, chips, pills, the Translated/Original toggle |
| caption | Fraunces 9pt Regular | 13 | | sizes, secondary row text, hints, version, zoom readout, license text |
| overline | Fraunces 9pt SemiBold | 11 | uppercase, tracking 0.8 | FROM/TO, card section headers, dropdown titles |

- Android: `ui/theme/Type.kt` (`AppType`). `AppTypography` maps every M3 style to a token (display* =
  display, headline*/titleLarge = title, titleMedium/Small = headline, bodyLarge/Medium = body,
  bodySmall = caption, labelLarge = button, labelMedium = label, labelSmall = overline), so stock
  components render in Fraunces.
- iOS: `Theme/Typography.swift` (`Typography`). Tokens use `Font.custom(_:size:relativeTo:)` so
  Dynamic Type scales them (display→title, title→title2, headline→headline, body/button→body,
  label→subheadline, caption→caption, overline→caption2). Navigation bars get the title token via
  `UINavigationBarAppearance`. System controls that ignore custom fonts (`.alert`, `Menu`, `Picker`,
  `Form`) are not used for app text; OS permission prompts are the only exception. Fraunces' natural
  line height is 1.23x, so iOS adds line spacing to match Android's token line heights (headline +1,
  body +2.3, caption +2). SF Symbols scale with Dynamic Type.
- **Photo text exception:** translated text and peeked original text drawn on the photo use the
  system font at Medium on both platforms, sized to the box. Fraunces only covers Latin, and photo
  text can be CJK, Arabic, Devanagari, etc.

## Color

The translatte (light) and americano (dark) palettes live in code: Android `ui/theme/Theme.kt`, iOS
`Assets.xcassets` color sets read through `Theme/Palette.swift`. Sheets, dialogs and dropdowns use the
sheet color; cards use surface; screens use background. Camera controls use a fixed espresso scrim
(#16110E at 55-60%) with cream text in both themes.

## Shape and size

| | value |
|---|---|
| card radius (cards, language bar, banners, glass cards over photos) | 20 |
| inner radius (FROM/TO tiles, photo inside a card) | 14 |
| dropdown radius | 16 on Android (iOS popovers use the system radius) |
| sheet radius | 28 (top corners) |
| dialog radius | 28 |
| buttons, chips, pills, toggles | capsule |
| full button height | 56 |
| small button height | 36 |
| chip height (direction chips, camera chip) | 44 |
| list row min height | 52 (dropdown rows 48); rows grow with larger text |
| screen gutter | 16 |
| spacing between blocks and cards | 16 |
| icon buttons | 48 Android / 44 iOS (each platform's touch target) |
| icons | 24 standalone, ~18 inside buttons |

## Components

- **Buttons:** filled primary (main action), tonal primary container (secondary full-size action),
  outlined with the outline color (Languages, Delete, Cancel), text in primary (dialog actions, Back
  on the error card). Full-size buttons use the button token; small ones the label token.
- **Chips and pills:** capsule, label token, 44 tall; glass fill over photos, espresso scrim over the
  camera. The Translated/Original toggle is a glass capsule with a primary-filled selected segment.
- **Cards:** surface color, card radius, overline header inside (16 from the left, 14 from the top);
  rows inside pad themselves. No dividers; rows are separated by their padding.
- **Rows:** min 52, 16 horizontal and 10 vertical padding, body title with an optional caption
  below, optional trailing icon or button. Selection is a primary checkmark on the trailing edge.
- **Language pickers:** tapping FROM or TO opens a compact dropdown from that chip, never covering it.
  Width is the chip's width clamped to 220-280, height at most 60% of the screen, scrolling inside,
  opened with the current choice as the third row. Overline title ("Translate from" / "Translate to"),
  rows at least 48 tall in the body token, checkmark on the current language. From lists installed
  sources only, To lists every language, alphabetical. Choosing one closes it.
  - Android: an anchored `Popup` (elevation 8), 6 below the chip and aligned to its leading edge for
    FROM and trailing edge for TO. It opens above when it doesn't fit below and there is more room
    above, and its height is capped to the room on that side.
  - iOS: a `.popover` kept as a popover on iPhone, with the system placement: centered on its arrow
    and clamped to the screen edges. The app asks for below, or above when the chip is in the lower
    half of its window (the camera's language sheet); on iOS 17 the system may still pick the side.
- **Sheets:** sheet color, 28 top corners, title token heading, 16 gutter (the camera's language sheet).
- **Dialogs:** sheet color, 28 radius, 24 padding, title token, body token in the muted text color,
  text buttons in primary aligned right (wrapping onto their own lines when they don't fit). Android:
  `AlertDialog` through `AppDialog`; iOS: the `AppDialog` overlay hosted over the whole window, which
  takes VoiceOver focus, hides the content behind it and closes on Escape. It sits under presented
  sheets, so dialogs must not be shown from inside a sheet.
- **Top bars (Settings, Languages):** page background, back affordance at the leading edge, title
  token next to it. Back is an arrow on Android and a chevron on iOS.
