import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { automation, AUTOMATION_EVENT, KIND_COLORS, timeOf, type AutomationEvent, type PlayLogEntry } from "./api";

const S: Record<string, React.CSSProperties> = {
  wrap: { display: "flex", flexDirection: "column", height: "100%", minHeight: 0 },
  head: {
    padding: "6px 12px", fontSize: 11, fontWeight: 700, letterSpacing: 0.6, textTransform: "uppercase",
    color: "var(--text-secondary)", borderBottom: "1px solid var(--border)", borderTop: "1px solid var(--border)",
  },
  list: { flex: 1, overflowY: "auto", fontSize: 11, fontFamily: "var(--font-mono)" },
  row: { display: "flex", gap: 6, padding: "2px 12px", whiteSpace: "nowrap" },
  time: { color: "var(--text-muted)" },
  text: { overflow: "hidden", textOverflow: "ellipsis" },
};

interface Line { at: string; text: string; color: string; key: string }

/** RadioBOSS-style log: what aired, and automation warnings in red. */
export default function LogPane() {
  const { t } = useTranslation();
  const [lines, setLines] = useState<Line[]>([]);

  const load = useCallback(async () => {
    try {
      const [plays, status] = await Promise.all([automation.log(undefined, undefined, 60), automation.status()]);
      const played: Line[] = plays.map((p: PlayLogEntry) => ({
        at: p.aired_at,
        text: `${p.artist ? `${p.artist} - ` : ""}${p.title}${p.complete ? "" : ` (${t(`automation.reasons.${p.reason}`)})`}`,
        color: KIND_COLORS[p.kind] ?? "var(--text-primary)",
        key: `p${p.id}`,
      }));
      const warned: Line[] = status.warnings.map(([at, w], i) => ({ at, text: w, color: "var(--red)", key: `w${i}${at}` }));
      setLines([...played, ...warned].sort((a, b) => b.at.localeCompare(a.at)).slice(0, 100));
    } catch { /* backend not ready */ }
  }, [t]);

  useEffect(() => {
    load();
    const off = listen<AutomationEvent>(AUTOMATION_EVENT, (e) => {
      if (e.payload.logged > 0 || e.payload.warnings.length > 0) load();
    });
    return () => { off.then((f) => f()); };
  }, [load]);

  return (
    <div style={S.wrap}>
      <div style={S.head}>{t("automation.logPane.title")}</div>
      <div style={S.list} role="log">
        {lines.length === 0 && <div style={{ ...S.row, color: "var(--text-muted)" }}>{t("automation.logPane.empty")}</div>}
        {lines.map((l) => (
          <div key={l.key} style={S.row}>
            <span style={S.time}>{timeOf(l.at)}</span>
            <span style={{ ...S.text, color: l.color }} title={l.text}>{l.text}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
