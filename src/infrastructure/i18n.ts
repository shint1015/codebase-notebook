// UI translations. The language follows the OS locale by default and can be
// overridden in settings (persisted in localStorage — it's a UI preference,
// not app data).
import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "../locales/en.json";
import ja from "../locales/ja.json";

export const LANGUAGES = { en: "English", ja: "日本語" } as const;
export type Language = keyof typeof LANGUAGES;

const STORAGE_KEY = "cbnb.language";

export function storedLanguage(): Language | null {
  const value = localStorage.getItem(STORAGE_KEY);
  return value === "en" || value === "ja" ? value : null;
}

function detectLanguage(): Language {
  const stored = storedLanguage();
  if (stored) return stored;
  return navigator.language.toLowerCase().startsWith("ja") ? "ja" : "en";
}

export function setLanguage(language: Language) {
  localStorage.setItem(STORAGE_KEY, language);
  void i18n.changeLanguage(language);
}

void i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, ja: { translation: ja } },
  lng: detectLanguage(),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

export default i18n;
