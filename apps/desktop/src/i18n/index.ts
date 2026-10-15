import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en.json";
import vi from "./locales/vi.json";

export type Language = "en" | "vi";

export const LANGUAGES: readonly { code: Language; label: string }[] = [
  { code: "en", label: "English" },
  { code: "vi", label: "Tiếng Việt" },
];

export const resources = {
  vi: { translation: vi },
  en: { translation: en },
} as const;

export function detectLanguage(
  locale: string | undefined = globalThis.navigator?.language,
): Language {
  return locale?.toLowerCase().startsWith("vi") ? "vi" : "en";
}

export function initI18n(language: Language) {
  if (i18n.isInitialized) {
    void i18n.changeLanguage(language);
    return i18n;
  }
  void i18n.use(initReactI18next).init({
    resources,
    lng: language,
    fallbackLng: "en",
    interpolation: { escapeValue: false },
    returnNull: false,
  });
  return i18n;
}

export default i18n;
