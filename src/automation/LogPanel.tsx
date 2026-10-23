import { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import {
  automation, AUTOMATION_EVENT, fmtDuration, today, ITEM_KINDS,
} from "./api";
import type { AutomationEvent, ItemKind, PlayLogEntry } from "./api";
import { KindBadge } from "./CategoriesPanel";

const LIMIT = 1000;

const pad = (n: number) => n.toString().padStart(2, "0");

const toInput = (d: Date) =>
  `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;

const toBackend = (value: string) => (value.length === 16 ? `${value}:00` : value.slice(0, 19));

const endsNow = (value: string) => {
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return false;
  const diff = Date.now() - d.getTime();
  return diff >= 0 && diff < 120000;
};

const aired = (value: string) => value.slice(5, 19).replace("T", " ");

const S: Record<string, React.CSSProperties> = {
  panel: { display: "flex", flexDirection: "column", height: "100%" },
  toolbar: {
    display: "flex", alignItems: "flex-end", gap: 8, padding: "8px 16px",
    borderBottom: "1px solid var(--border)", flexWrap: "wrap" as const,
  },
  title: { fontSize: 14, fontWeight: 600, alignSelf: "center", marginRight: 8 },
  field: { display: "flex", flexDirection: "column", gap: 2, fontSize: 11, color: "var(--text-secondary)" },
  input: { fontSize: 12 },
  btn: { padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12, background: "var(--bg-panel)" },
  error: { padding: "6px 16px", fontSize: 12, color: "var(--red)" },
  notice: { padding: "6px 16px", fontSize: 12, color: "var(--green)" },
  tableWrap: { flex: 1, overflow: "auto", minHeight: 0 },
  table: { width: "100%", borderCollapse: "collapse" as const, fontSize: 12 },
  th: {
    textAlign: "left" as const, padding: "5px 8px", fontSize: 10, fontWeight: 700,
    textTransform: "uppercase" as const, letterSpacing: 0.5, color: "var(--text-muted)",
    borderBottom: "1px solid var(--border)", position: "sticky" as const, top: 0,
    background: "var(--bg-primary)",
  },
  td: { padding: "4px 8px", borderBottom: "1px solid rgba(255,255,255,0.04)" },
  empty: { padding: "24px 16px", textAlign: "center", color: "var(--text-muted)", fontSize: 12 },
  foot: {
    display: "flex", alignItems: "center", gap: 8, padding: "6px 16px",
    borderTop: "1px solid var(--border)", fontSize: 11, color: "var(--text-muted)",
  },
  cut: { color: "var(--yellow)" },
  ok: { color: "var(--green)" },
  bad: { color: "var(--red)" },
};

export default function LogPanel() {
  const { t } = useTranslation();
  const [from, setFrom] = useState(() => toInput(new Date(Date.now() - 24 * 3600 * 1000)));
  const [to, setTo] = useState(() => toInput(new Date()));
  const [kind, setKind] = useState<ItemKind | "all">("all");
  const [text, setText] = useState("");
  const [entries, setEntries] = useState<PlayLogEntry[]>([]);
  const [cut, setCut] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const range = useRef({ from: toBackend(from), to: toBackend(to) });
  range.current = { from: toBackend(from), to: toBackend(to) };

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const list = await automation.log(
        range.current.from || undefined,
        range.current.to || undefined,
        LIMIT + 1,
      );
      setEntries(list.slice(0, LIMIT));
      setCut(list.length > LIMIT);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  useEffect(() => {
    const unlisten = listen<AutomationEvent>(AUTOMATION_EVENT, (event) => {
      if (event.payload.logged > 0 && endsNow(range.current.to)) load();
    });
    return () => { unlisten.then((f) => f()); };
  }, [load]);

  const exportLog = async () => {
    try {
      const path = await save({
        defaultPath: `riars-log-${today()}.csv`,
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (!path) return;
      const bytes = await automation.exportCsv("log", path, {
        from: range.current.from || undefined,
        to: range.current.to || undefined,
      });
      setNotice(t("automation.log.exported", { bytes }));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const needle = text.trim().toLowerCase();
  const filtered = entries.filter((entry) => {
    if (kind !== "all" && entry.kind !== kind) return false;
    if (needle === "") return true;
    return entry.title.toLowerCase().includes(needle) || entry.artist.toLowerCase().includes(needle);
  });

  return (
    <div style={S.panel}>
      <div style={S.toolbar}>
        <h2 style={S.title}>{t("automation.log.title")}</h2>
        <label style={S.field}>{t("automation.log.from")}
          <input style={S.input} type="datetime-local" step={1} value={from}
            onChange={(e) => setFrom(e.target.value)} />
        </label>
        <label style={S.field}>{t("automation.log.to")}
          <input style={S.input} type="datetime-local" step={1} value={to}
            onChange={(e) => setTo(e.target.value)} />
        </label>
        <label style={S.field}>{t("automation.log.kindFilter")}
          <select style={S.input} value={kind} onChange={(e) => setKind(e.target.value as ItemKind | "all")}>
            <option value="all">{t("automation.log.allKinds")}</option>
            {ITEM_KINDS.map((k) => (
              <option key={k} value={k}>{t(`automation.kinds.${k}`, k)}</option>
            ))}
          </select>
        </label>
        <label style={{ ...S.field, flex: 1, minWidth: 140 }}>{t("automation.log.textFilter")}
          <input style={S.input} type="search" placeholder={t("automation.log.textPlaceholder")}
            value={text} onChange={(e) => setText(e.target.value)} />
        </label>
        <button style={S.btn} onClick={load} disabled={loading}>
          {loading ? t("automation.log.loading") : t("automation.log.refresh")}
        </button>
        <button style={S.btn} onClick={exportLog}>{t("automation.log.export")}</button>
      </div>

      {error && <div style={S.error}>{error}</div>}
      {notice && <div style={S.notice}>{notice}</div>}

      <div style={S.tableWrap}>
        <table style={S.table}>
          <thead>
            <tr>
              <th style={S.th}>{t("automation.log.columnAired")}</th>
              <th style={S.th}>{t("automation.log.columnKind")}</th>
              <th style={S.th}>{t("automation.log.columnTitle")}</th>
              <th style={S.th}>{t("automation.log.columnArtist")}</th>
              <th style={{ ...S.th, textAlign: "right" as const }}>{t("automation.log.columnDuration")}</th>
              <th style={S.th}>{t("automation.log.columnComplete")}</th>
              <th style={S.th}>{t("automation.log.columnCampaign")}</th>
            </tr>
          </thead>
          <tbody>
            {filtered.length === 0 && (
              <tr><td colSpan={7} style={{ ...S.td, textAlign: "center", color: "var(--text-muted)" }}>
                {t("automation.log.empty")}
              </td></tr>
            )}
            {filtered.map((entry) => (
              <tr key={entry.id}>
                <td style={{ ...S.td, fontFamily: "var(--font-mono)", whiteSpace: "nowrap" }}>
                  {aired(entry.aired_at)}
                </td>
                <td style={S.td}><KindBadge kind={entry.kind} /></td>
                <td style={S.td}>{entry.title}</td>
                <td style={{ ...S.td, color: "var(--text-secondary)" }}>{entry.artist}</td>
                <td style={{ ...S.td, textAlign: "right" as const, fontFamily: "var(--font-mono)" }}>
                  {fmtDuration(entry.played_ms)}
                </td>
                <td style={S.td}>
                  {entry.complete ? (
                    <span style={S.ok} title={t("automation.log.complete")}>&#10003;</span>
                  ) : (
                    <span style={S.bad} title={t(`automation.log.reasons.${entry.reason}`, entry.reason)}>
                      &#10007; {t(`automation.log.reasons.${entry.reason}`, entry.reason)}
                    </span>
                  )}
                </td>
                <td style={{ ...S.td, fontFamily: "var(--font-mono)", color: "var(--text-muted)" }}>
                  {entry.campaign_id ?? ""}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div style={S.foot}>
        <span>{t("automation.log.count", { count: filtered.length })}</span>
        {cut && <span style={S.cut}>{t("automation.log.truncated", { limit: LIMIT })}</span>}
      </div>
    </div>
  );
}
