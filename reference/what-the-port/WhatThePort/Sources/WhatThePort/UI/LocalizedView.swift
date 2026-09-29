import SwiftUI

/// Rebuild presentation when the preference changes; the shared monitor retains
/// its scans and process state. Also applies explicit Hebrew layout direction.
struct LocalizedView<Content: View>: View {
    @AppStorage(Preferences.language) private var language = InterfaceLanguage.system.rawValue
    @ViewBuilder var content: () -> Content

    var body: some View {
        content()
            .id(language)
            .environment(\.locale, L10n.locale)
            .environment(\.layoutDirection, L10n.language.isRightToLeft ? .rightToLeft : .leftToRight)
    }
}
