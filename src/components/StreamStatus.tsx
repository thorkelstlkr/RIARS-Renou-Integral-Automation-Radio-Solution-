import { useTranslation } from "react-i18next";

const S: Record<string, React.CSSProperties> = {
  wrap: {
    display: "flex", alignItems: "center", gap: 6, marginLeft: "auto",
    padding: "6px 12px", borderRadius: "var(--radius)", background: "var(--bg-input)",
  },
  dot: { width: 8, height: 8, borderRadius: "50%" },
  label: { fontSize: 11, fontWeight: 700, letterSpacing: 0.5 },
};

export default function StreamStatus() {
  const { t } = useTranslation();
  const connected = false;
  return (
    <div style={S.wrap}>
      <span style={{ ...S.dot, background: connected ? "var(--green)" : "var(--text-muted)" }} />
      <span style={S.label}>{connected ? t("stream.streaming") : t("stream.offline")}</span>
    </div>
  );
}
