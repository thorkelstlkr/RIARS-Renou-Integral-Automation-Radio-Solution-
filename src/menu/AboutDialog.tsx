import { useTranslation } from "react-i18next";
import Dialog from "./Dialog";

/** Help ▸ About RIARS. */
export default function AboutDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  return (
    <Dialog title={t("menu.aboutTitle")} onClose={onClose}>
      <div style={{ textAlign: "center", lineHeight: 1.7 }}>
        <img src="/logo-128.png" alt="" width={96} height={96} style={{ marginBottom: 6 }} />
        <div style={{ fontSize: 28, fontWeight: 800, color: "var(--accent)" }}>RIARS</div>
        <div style={{ color: "var(--text-secondary)" }}>{t("app.subtitle")}</div>
        <div style={{ fontFamily: "var(--font-mono)", margin: "8px 0" }}>{t("app.version")}</div>
        <p style={{ fontSize: 13, color: "var(--text-secondary)", margin: "12px 0" }}>{t("menu.aboutText")}</p>
        <p style={{ fontSize: 12, color: "var(--text-muted)" }}>{t("menu.aboutLicense")}</p>
      </div>
    </Dialog>
  );
}
