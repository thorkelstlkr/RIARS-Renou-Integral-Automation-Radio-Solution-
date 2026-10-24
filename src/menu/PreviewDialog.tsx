import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { automation, fmtDuration, KIND_COLORS, timeOf, type PlaylistItem } from "../automation/api";
import Dialog from "./Dialog";

const S: Record<string, React.CSSProperties> = {
  bar: { display: "flex", alignItems: "center", gap: 8, marginBottom: 10, fontSize: 13 },
  hint: { fontSize: 12, color: "var(--text-muted)", flex: 1 },
  table: { width: "100%", borderCollapse: "collapse", fontSize: 12 },
  th: { textAlign: "left", padding: "4px 6px", color: "var(--text-muted)", fontWeight: 600, borderBottom: "1px solid var(--border)" },
  td: { padding: "3px 6px", borderBottom: "1px solid rgba(255,255,255,0.03)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", maxWidth: 260 },
  mono: { fontFamily: "var(--font-mono)" },
  error: { color: "var(--red)", fontSize: 12 },
};

/** Automation ▸ Preview…: what the automation would put on air, without changing anything. */
export default function PreviewDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [hours, setHours] = useState(2);
  const [items, setItems] = useState<PlaylistItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setItems(null);
    automation.preview(hours).then(setItems).catch((e) => setError(String(e)));
  }, [hours]);

  return (
    <Dialog title={t("menu.previewTitle")} width={760} onClose={onClose}>
      <div style={S.bar}>
        <label>
          {t("menu.previewHours")}{" "}
          <select value={hours} onChange={(e) => setHours(Number(e.target.value))}>
            {[1, 2, 6, 12, 24, 48].map((h) => <option key={h} value={h}>{h}</option>)}
          </select>
        </label>
        <span style={S.hint}>{t("menu.previewHint")}</span>
      </div>
      {error && <div style={S.error}>{error}</div>}
      {!items && !error && <div style={S.hint}>{t("automation.log.loading")}</div>}
      {items && (
        <table style={S.table}>
          <thead>
            <tr>
              <th style={S.th}>{t("playlist.headerStart")}</th>
              <th style={S.th}>{t("automation.log.columnKind")}</th>
              <th style={S.th}>{t("playlist.headerTitle")}</th>
              <th style={S.th}>{t("playlist.headerArtist")}</th>
              <th style={{ ...S.th, textAlign: "right" }}>{t("playlist.headerDuration")}</th>
            </tr>
          </thead>
          <tbody>
            {items.map((item) => (
              <tr key={item.id}>
                <td style={{ ...S.td, ...S.mono }}>{timeOf(item.planned_at)}</td>
                <td style={{ ...S.td, color: KIND_COLORS[item.kind ?? "Music"] }}>{t(`automation.kinds.${item.kind ?? "Music"}`)}</td>
                <td style={S.td} title={item.track_path}>{item.title}</td>
                <td style={{ ...S.td, color: "var(--text-secondary)" }}>{item.artist}</td>
                <td style={{ ...S.td, ...S.mono, textAlign: "right" }}>{fmtDuration(item.duration_ms)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {items && items.length === 0 && <div style={S.hint}>{t("menu.previewEmpty")}</div>}
    </Dialog>
  );
}
