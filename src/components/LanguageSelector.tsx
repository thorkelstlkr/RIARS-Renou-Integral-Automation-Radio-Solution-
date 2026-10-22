import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

const LANGS = ["en", "es", "fr", "de", "pt", "ca"] as const;

const S: Record<string, React.CSSProperties> = {
  wrap: { display: "flex", alignItems: "center", gap: 6 },
  select: { fontSize: 12, padding: "4px 8px", cursor: "pointer" },
};

export default function LanguageSelector() {
  const { t, i18n } = useTranslation();

  const handleChange = (lang: string) => {
    i18n.changeLanguage(lang);
    invoke("set_locale", { lang }).catch(() => {});
  };

  return (
    <div style={S.wrap}>
      <select
        style={S.select}
        value={i18n.language}
        onChange={(e) => handleChange(e.target.value)}
        title={t("language.label")}
      >
        {LANGS.map((lng) => (
          <option key={lng} value={lng}>
            {t(`language.${lng}`)}
          </option>
        ))}
      </select>
    </div>
  );
}
