import APP_STRINGS from './appStrings.json'

// Keep these stable codes aligned with InterfaceLanguage in Localization.swift.
export const APP_LANGUAGES = [
  { code: 'en', label: 'English', name: 'English' },
  { code: 'de', label: 'Deutsch', name: 'German' },
  { code: 'fr', label: 'Français', name: 'French' },
  { code: 'es', label: 'Español', name: 'Spanish' },
  { code: 'zh-Hans', label: '简体中文', name: 'Simplified Chinese' },
  { code: 'he', label: 'עברית', name: 'Hebrew' },
  { code: 'ja', label: '日本語', name: 'Japanese' },
  { code: 'uk', label: 'Українська', name: 'Ukrainian' },
] as const

export type AppLanguageCode = 'system' | (typeof APP_LANGUAGES)[number]['code']
export const appLanguageLabel = (code: AppLanguageCode) =>
  APP_LANGUAGES.find((language) => language.code === code)?.label ?? 'System default'

// A port of L10n in Localization.swift, over the app's own strings (scripts/app-strings.mjs).
// English phrases are the keys and the fallback.
const TABLES: Record<string, Record<string, string>> = APP_STRINGS

const fill = (template: string, args: (string | number)[]) => {
  let next = 0
  return template.replace(/%(?:(\d+)\$)?([@d%])/g, (_, position: string | undefined, type: string) =>
    type === '%' ? '%' : String(args[position ? Number(position) - 1 : next++]),
  )
}

export type Localizer = ReturnType<typeof localizer>

export function localizer(code: AppLanguageCode) {
  const table = code === 'system' || code === 'en' ? undefined : TABLES[code]
  const text = (english: string) => table?.[english] ?? english
  const format = (english: string, ...args: (string | number)[]) => fill(text(english), args)
  return {
    code,
    rtl: code === 'he',
    text,
    format,
    // Ukrainian has one/few/many noun forms, including 21 and 22–24.
    counted(singular: string, plural: string, count: number) {
      if (code === 'uk') {
        const last = count % 10
        const lastTwo = count % 100
        if (last === 1 && lastTwo !== 11) return text(singular)
        if (last >= 2 && last <= 4 && !(lastTwo >= 12 && lastTwo <= 14)) return text(`${plural} (few)`)
      }
      return text(count === 1 || (code === 'fr' && count === 0) ? singular : plural)
    },
    // GUI durations, from whole minutes: Format.duration / shortDuration in English.
    duration(minutes: number, short = false) {
      if (minutes < 1) return short ? format('%dm', 1) : text('<1m')
      if (minutes < 60) return format('%dm', minutes)
      const hours = Math.floor(minutes / 60)
      if (hours >= 24) return format('%dd', Math.floor(hours / 24))
      const rest = minutes % 60
      return short || rest === 0 ? format('%dh', hours) : format('%dh %dm', hours, rest)
    },
  }
}
