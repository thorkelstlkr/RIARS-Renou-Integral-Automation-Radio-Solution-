import i18n from "i18next";
import { initReactI18next } from "react-i18next";

import en from "./locales/en.json";
import es from "./locales/es.json";
import fr from "./locales/fr.json";
import de from "./locales/de.json";
import pt from "./locales/pt.json";
import ca from "./locales/ca.json";

const STORAGE_KEY = "riars-language";

function getInitialLanguage(): string {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && ["en", "es", "fr", "de", "pt", "ca"].includes(saved)) return saved;
  } catch { /* localStorage unavailable */ }
  const nav = navigator.language.split("-")[0];
  if (["en", "es", "fr", "de", "pt", "ca"].includes(nav)) return nav;
  return "en";
}

i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, es: { translation: es }, fr: { translation: fr }, de: { translation: de }, pt: { translation: pt }, ca: { translation: ca } },
  lng: getInitialLanguage(),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

i18n.on("languageChanged", (lng) => {
  try { localStorage.setItem(STORAGE_KEY, lng); } catch { /* */ }
});

export default i18n;
