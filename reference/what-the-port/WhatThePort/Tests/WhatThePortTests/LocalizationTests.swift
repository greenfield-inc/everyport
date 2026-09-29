import Foundation
import Testing
@testable import WhatThePort

struct LocalizationTests {
    @Test func resolvesSavedAndSystemLanguages() {
        let cases: [([String], InterfaceLanguage)] = [
            (["zh-Hans-US"], .simplifiedChinese), (["zh_CN"], .simplifiedChinese),
            (["zh-TW", "fr-CA"], .french), (["zh-Hant"], .english),
            (["de-DE"], .german), (["fr-FR"], .french), (["es-MX"], .spanish),
            (["he-IL"], .hebrew), (["iw-IL"], .hebrew),
            (["ko-KR", "de-AT", "en-US"], .german), (["ja-JP"], .japanese), (["uk-UA"], .ukrainian), (["en-US", "de-DE"], .english),
            ([], .english), (["unknown"], .english),
        ]
        for (preferred, expected) in cases {
            #expect(InterfaceLanguage.system.resolved(preferredLanguages: preferred) == expected)
        }
        for language in InterfaceLanguage.allCases where language != .system {
            #expect(language.resolved(preferredLanguages: ["en-US"]) == language)
        }
        #expect(InterfaceLanguage.hebrew.isRightToLeft)
        #expect(!InterfaceLanguage.german.isRightToLeft)
    }

    @Test func loadsAllLanguagesAndPreservesUnknownUserContent() {
        let labels: [(InterfaceLanguage, String)] = [(.english, "Settings"), (.german, "Einstellungen"),
            (.french, "Réglages"), (.spanish, "Ajustes"), (.simplifiedChinese, "设置"), (.hebrew, "הגדרות"), (.japanese, "設定"), (.ukrainian, "Налаштування")]
        for (language, expected) in labels {
            #expect(L10n.text("Settings", language: language) == expected)
            let content = "~/פרויקט/中文/café · npm run dev · feature/Größe"
            #expect(L10n.text(content, language: language) == content)
        }
        #expect(String(format: L10n.text("Stop %d processes", language: .simplifiedChinese), 3) == "停止 3 个进程")
        #expect(String(format: L10n.text("+%@ in %@", language: .simplifiedChinese), "500 MB", "10 分钟") == "10 分钟内增加 500 MB")
        #expect(String(format: L10n.text("%@ grew %@ in %@ and is now using %@.", language: .german),
                       "api", "500 MB", "10 Min.", "2 GB") == "api ist in 10 Min. um 500 MB gewachsen und belegt jetzt 2 GB.")
    }

    @Test func resourceTablesHaveMatchingKeysAndArgumentPositions() throws {
        let english = try table(.english)
        for language in InterfaceLanguage.allCases where language != .system {
            let translated = try table(language)
            #expect(Set(english.keys) == Set(translated.keys), "Key coverage: \(language)")
            for (key, value) in english {
                let translation = try #require(translated[key])
                #expect(!translation.isEmpty, "Empty \(language): \(key)")
                #expect(value == key)
                #expect(try arguments(value) == arguments(translation), "Argument positions: \(language): \(key)")
            }
        }
    }

    @Test func ukrainianCountFormsIncludeTeensAndCompoundNumbers() {
        for count in [1, 21, 101] { #expect(L10n.counted("server", "servers", count: count, language: .ukrainian) == "сервер") }
        for count in [2, 3, 4, 22, 24] { #expect(L10n.counted("server", "servers", count: count, language: .ukrainian) == "сервери") }
        for count in [0, 5, 11, 12, 14, 25, 111] { #expect(L10n.counted("server", "servers", count: count, language: .ukrainian) == "серверів") }
        #expect(L10n.counted("tool", "tools", count: 4, language: .ukrainian) == "інструменти")
    }

    @Test func hebrewKeepsTechnicalTokensInReadingOrder() {
        #expect(L10n.text("Open localhost:%@", language: .hebrew).contains("\u{2066}localhost:%@\u{2069}"))
        #expect(L10n.text("Show popover with ⌥⌘P", language: .hebrew).contains("\u{2066}⌥⌘P\u{2069}"))
        #expect(TextWidth.of("\u{2066}:3000\u{2069}") == 5)
    }

    @Test func terminalHandlesTranslatedProjectNamesWithoutSplittingCharacters() {
        for name in ["Größe", "développement", "中文服务", "שָׁלוֹם", "proyecto", "日本語", "한국어", "Українська"] {
            let input = TerminalScreen.parse(Array(name.utf8))
            #expect(input == name.map(InputEvent.character))
            for width in 0...12 {
                let line = [Span(name)].fitted(to: width)
                #expect(line.width == width)
                #expect(TextWidth.of(TextWidth.middle(name, width: width)) <= width)
            }
        }
        #expect(TextWidth.of("中文") == 4)
        #expect(TextWidth.of("שָׁלוֹם") == 4)
    }

    private func table(_ language: InterfaceLanguage) throws -> [String: String] {
        let name = try #require(L10n.resourceBundle.localizations.first {
            $0.caseInsensitiveCompare(language.rawValue) == .orderedSame
        })
        let url = L10n.resourceBundle.bundleURL.appendingPathComponent("\(name).lproj/Localizable.strings")
        return try #require(PropertyListSerialization.propertyList(from: Data(contentsOf: url), options: [], format: nil) as? [String: String])
    }

    /// Validate positions as well as types: swapping %@ and %d must fail, while
    /// Chinese/German can intentionally reorder explicitly numbered arguments.
    private func arguments(_ value: String) throws -> [String] {
        let regex = try NSRegularExpression(pattern: "%([0-9]+\\$)?([@df])")
        return regex.matches(in: value, range: NSRange(value.startIndex..., in: value)).enumerated().map { index, match in
            let position = Range(match.range(at: 1), in: value).map { String(value[$0].dropLast()) } ?? String(index + 1)
            let type = String(value[Range(match.range(at: 2), in: value)!])
            return "\(position):\(type)"
        }.sorted()
    }
}
