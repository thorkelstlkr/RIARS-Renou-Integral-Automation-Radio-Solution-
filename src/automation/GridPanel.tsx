import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { automation } from "./api";
import type { Clock, GridRule, GridTarget } from "./api";
import { dateKey, resolveGrid } from "./grid";

interface SavedPlaylist {
  id: string;
  name: string;
  items: number;
  duration_ms: number;
}

const DAYS = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
const MONTHS = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
const WEEKS = ["week1", "week2", "week3", "week4", "week5", "last"];

const pad2 = (n: number) => n.toString().padStart(2, "0");
const clone = <T,>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

const mondayOf = (date: Date): Date => {
  const d = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const weekday = (d.getDay() + 6) % 7;
  d.setDate(d.getDate() - weekday);
  return d;
};

const addDays = (date: Date, days: number): Date => {
  const d = new Date(date);
  d.setDate(d.getDate() + days);
  return d;
};

const parseDate = (value: string): Date | null => {
  const [y, m, d] = value.split("-").map(Number);
  if (!y || !m || !d) return null;
  return new Date(y, m - 1, d);
};

const normalizeTime = (value: string): string => {
  if (!value) return "00:00:00";
  return value.length === 5 ? `${value}:00` : value.slice(0, 8);
};

const slugify = (name: string) =>
  name
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "") || "rule";

const makeId = (name: string, rules: GridRule[]) => {
  const base = slugify(name);
  if (!rules.some((rule) => rule.id === base)) return base;
  let id = base;
  do {
    id = `${base}-${Math.random().toString(36).slice(2, 6)}`;
  } while (rules.some((rule) => rule.id === id));
  return id;
};

const S: Record<string, React.CSSProperties> = {
  panel: { display: "flex", flexDirection: "column", height: "100%" },
  header: {
    display: "flex", alignItems: "center", gap: 8, padding: "10px 16px",
    borderBottom: "1px solid var(--border)",
  },
  title: { fontSize: 14, fontWeight: 600, flex: 1 },
  weekBar: { display: "flex", alignItems: "center", gap: 6, marginLeft: "auto", fontSize: 12 },
  error: { padding: "6px 16px", fontSize: 12, color: "var(--red)", borderBottom: "1px solid var(--border)" },
  weekWrap: { flex: 1, minHeight: 0, overflow: "auto" },
  gridTable: { width: "100%", borderCollapse: "collapse", tableLayout: "fixed", fontSize: 11 },
  corner: {
    width: 48, position: "sticky", top: 0, left: 0, zIndex: 3,
    background: "var(--bg-secondary)", borderBottom: "1px solid var(--border)",
  },
  dayHead: {
    position: "sticky", top: 0, zIndex: 2, padding: "5px 4px", fontSize: 11, fontWeight: 600,
    background: "var(--bg-secondary)", borderBottom: "1px solid var(--border)",
    borderLeft: "1px solid rgba(255,255,255,0.04)",
  },
  hourCell: {
    width: 48, padding: "0 6px", fontFamily: "var(--font-mono)", fontWeight: 400, fontSize: 10,
    textAlign: "right", color: "var(--text-muted)",
    borderBottom: "1px solid rgba(255,255,255,0.04)",
  },
  cell: {
    height: 21, padding: "1px 4px", borderBottom: "1px solid rgba(255,255,255,0.04)",
    borderLeft: "1px solid rgba(255,255,255,0.04)", overflow: "hidden",
  },
  cellInner: { display: "flex", alignItems: "center", gap: 4, overflow: "hidden" },
  cellDot: { width: 6, height: 6, borderRadius: 3, flexShrink: 0 },
  cellText: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: 10 },
  bottom: {
    flexShrink: 0, height: 320, maxHeight: "55%", display: "flex", minHeight: 0,
    borderTop: "1px solid var(--border)", background: "var(--bg-secondary)",
  },
  ruleList: {
    width: "42%", minWidth: 330, display: "flex", flexDirection: "column",
    borderRight: "1px solid var(--border)", minHeight: 0,
  },
  sectionHead: {
    display: "flex", alignItems: "center", gap: 8, padding: "8px 10px",
    borderBottom: "1px solid var(--border)", fontSize: 12, fontWeight: 600,
  },
  listScroll: { flex: 1, overflow: "auto", minHeight: 0 },
  ruleTable: { width: "100%", minWidth: 520, borderCollapse: "collapse", fontSize: 11 },
  th: {
    textAlign: "left", padding: "4px 6px", fontSize: 10, fontWeight: 600,
    textTransform: "uppercase", letterSpacing: 0.5, color: "var(--text-muted)",
    borderBottom: "1px solid var(--border)", whiteSpace: "nowrap",
  },
  td: {
    padding: "3px 6px", borderBottom: "1px solid rgba(255,255,255,0.04)",
    whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", maxWidth: 140,
  },
  ruleRow: { cursor: "pointer" },
  activeRow: { background: "var(--bg-panel)" },
  editor: { flex: 1, minWidth: 0, overflow: "auto", padding: 12 },
  fields: { display: "flex", flexWrap: "wrap", alignItems: "flex-end", gap: 10 },
  field: { display: "flex", flexDirection: "column", gap: 3, fontSize: 11, color: "var(--text-secondary)" },
  input: { fontSize: 12, padding: "3px 6px" },
  targetRow: { display: "flex", gap: 4 },
  checks: { display: "flex", flexWrap: "wrap", gap: 6 },
  group: { marginTop: 10 },
  groupLabel: {
    fontSize: 10, fontWeight: 600, textTransform: "uppercase", letterSpacing: 0.5,
    color: "var(--text-muted)", marginBottom: 4,
  },
  check: { display: "flex", alignItems: "center", gap: 3, fontSize: 11, color: "var(--text-secondary)" },
  hint: { marginTop: 10, fontSize: 11, color: "var(--text-muted)" },
  actions: { display: "flex", gap: 6, marginTop: 12 },
  btn: {
    padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12,
    background: "var(--bg-panel)", border: "1px solid var(--border)",
  },
  btnPrimary: {
    padding: "4px 12px", borderRadius: "var(--radius)", fontSize: 12,
    background: "var(--accent)", border: "1px solid var(--accent)", color: "#fff",
  },
  empty: { padding: "24px 16px", textAlign: "center", color: "var(--text-muted)", fontSize: 13 },
};

export default function GridPanel() {
  const { t, i18n } = useTranslation();
  const [rules, setRules] = useState<GridRule[]>([]);
  const [clocks, setClocks] = useState<Clock[]>([]);
  const [playlists, setPlaylists] = useState<SavedPlaylist[]>([]);
  const [defaultClockId, setDefaultClockId] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [draft, setDraft] = useState<GridRule | null>(null);
  const [original, setOriginal] = useState<string | null>(null);
  const [refDate, setRefDate] = useState<Date>(() => mondayOf(new Date()));
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (): Promise<GridRule[]> => {
    try {
      const [list, clockList, config] = await Promise.all([
        automation.grid(),
        automation.clocks(),
        automation.getConfig(),
      ]);
      setRules(list);
      setClocks(clockList);
      setDefaultClockId(config.default_clock);
      setError(null);
      try {
        setPlaylists(await invoke<SavedPlaylist[]>("playlists_list"));
      } catch (e) {
        setError(String(e));
      }
      return list;
    } catch (e) {
      setError(String(e));
      return [];
    }
  }, []);

  useEffect(() => {
    load().then((list) => {
      if (list.length > 0) {
        setSelected(list[0].id);
        setDraft(clone(list[0]));
        setOriginal(JSON.stringify(list[0]));
      }
    });
  }, [load]);

  const dirty = draft != null && (draft.id === "" || JSON.stringify(draft) !== original);

  const selectRule = (id: string) => {
    if (draft?.id === id) return;
    if (dirty && !window.confirm(t("automation.grid.confirmDiscard"))) return;
    const rule = rules.find((r) => r.id === id);
    if (!rule) return;
    setSelected(id);
    setDraft(clone(rule));
    setOriginal(JSON.stringify(rule));
    setError(null);
  };

  const newRule = () => {
    if (dirty && !window.confirm(t("automation.grid.confirmDiscard"))) return;
    const target: GridTarget = clocks.length > 0
      ? { Clock: { clock_id: clocks[0].id } }
      : { Playlist: { playlist_id: playlists[0]?.id ?? "" } };
    setSelected(null);
    setDraft({
      id: "",
      name: t("automation.grid.defaultName"),
      priority: 0,
      target,
      weekdays: 0x7f,
      months: 0xfff,
      weeks_of_month: 0x3f,
      start: "00:00:00",
      end: "00:00:00",
      date_from: null,
      date_to: null,
      enabled: true,
    });
    setOriginal(null);
    setError(null);
  };

  const save = async () => {
    if (!draft) return;
    const rule: GridRule = { ...draft, id: draft.id || makeId(draft.name, rules) };
    const targetId = "Clock" in rule.target
      ? rule.target.Clock.clock_id
      : rule.target.Playlist.playlist_id;
    if (!targetId) {
      setError(t("automation.grid.pickTarget"));
      return;
    }
    try {
      const saved = await automation.saveGridRule(rule);
      await load();
      setSelected(saved.id);
      setDraft(clone(saved));
      setOriginal(JSON.stringify(saved));
    } catch (e) {
      setError(String(e));
    }
  };

  const removeRule = async () => {
    if (!draft) return;
    if (draft.id && !window.confirm(t("automation.grid.confirmDelete", { name: draft.name || draft.id }))) {
      return;
    }
    try {
      if (draft.id) await automation.deleteGridRule(draft.id);
      const list = await load();
      const next = list.find((rule) => rule.id !== draft.id) ?? null;
      setSelected(next?.id ?? null);
      setDraft(next ? clone(next) : null);
      setOriginal(next ? JSON.stringify(next) : null);
    } catch (e) {
      setError(String(e));
    }
  };

  const shiftWeek = (weeks: number) => setRefDate(mondayOf(addDays(refDate, weeks * 7)));

  const targetName = (target: GridTarget) =>
    "Clock" in target
      ? clocks.find((clock) => clock.id === target.Clock.clock_id)?.name ?? target.Clock.clock_id
      : playlists.find((playlist) => playlist.id === target.Playlist.playlist_id)?.name
        ?? target.Playlist.playlist_id;

  const targetColor = (target: GridTarget) =>
    "Clock" in target
      ? clocks.find((clock) => clock.id === target.Clock.clock_id)?.color ?? "var(--blue)"
      : "var(--blue)";

  const daysLabel = (mask: number) => {
    if ((mask & 0x7f) === 0x7f) return t("automation.grid.allDays");
    const names = DAYS.filter((_, i) => (mask & (1 << i)) !== 0)
      .map((day) => t(`automation.grid.day.${day}`));
    return names.length > 0 ? names.join(", ") : t("automation.grid.noDays");
  };

  const datesLabel = (rule: GridRule) => {
    if (rule.date_from && rule.date_to) return `${rule.date_from} – ${rule.date_to}`;
    if (rule.date_from) return `${rule.date_from} – …`;
    if (rule.date_to) return `… – ${rule.date_to}`;
    return t("automation.grid.anyDate");
  };

  const setTargetKind = (kind: "Clock" | "Playlist") => {
    if (!draft) return;
    const id = kind === "Clock" ? clocks[0]?.id ?? "" : playlists[0]?.id ?? "";
    setDraft({
      ...draft,
      target: kind === "Clock"
        ? { Clock: { clock_id: id } }
        : { Playlist: { playlist_id: id } },
    });
  };

  const targetKind = draft && "Clock" in draft.target ? "Clock" : "Playlist";
  const targetId = draft
    ? "Clock" in draft.target
      ? draft.target.Clock.clock_id
      : draft.target.Playlist.playlist_id
    : "";

  const setTargetId = (id: string) => {
    if (!draft) return;
    setDraft({
      ...draft,
      target: targetKind === "Clock"
        ? { Clock: { clock_id: id } }
        : { Playlist: { playlist_id: id } },
    });
  };

  const weekStart = mondayOf(refDate);
  const weekDays = Array.from({ length: 7 }, (_, i) => addDays(weekStart, i));
  const dayFormat = new Intl.DateTimeFormat(i18n.language, { weekday: "short" });
  const defaultClock = defaultClockId
    ? clocks.find((clock) => clock.id === defaultClockId)
    : undefined;
  const defaultName = defaultClock?.name ?? defaultClockId;

  return (
    <div style={S.panel}>
      <div style={S.header}>
        <span style={S.title}>{t("automation.grid.title")}</span>
        <div style={S.weekBar}>
          <span>{t("automation.grid.week")}</span>
          <button type="button" style={S.btn} title={t("automation.grid.previousWeek")}
            aria-label={t("automation.grid.previousWeek")} onClick={() => shiftWeek(-1)}>
            ‹
          </button>
          <input
            type="date"
            value={dateKey(weekStart)}
            aria-label={t("automation.grid.week")}
            style={S.input}
            onChange={(e) => {
              const date = parseDate(e.target.value);
              if (date) setRefDate(mondayOf(date));
            }}
          />
          <button type="button" style={S.btn} title={t("automation.grid.nextWeek")}
            aria-label={t("automation.grid.nextWeek")} onClick={() => shiftWeek(1)}>
            ›
          </button>
        </div>
      </div>
      {error && <div style={S.error}>{error}</div>}

      <div style={S.weekWrap}>
        <table style={S.gridTable}>
          <thead>
            <tr>
              <th style={S.corner} />
              {weekDays.map((day) => (
                <th key={dateKey(day)} style={S.dayHead} title={dateKey(day)}>
                  {dayFormat.format(day)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {Array.from({ length: 24 }, (_, hour) => (
              <tr key={hour}>
                <th style={S.hourCell}>{pad2(hour)}:00</th>
                {weekDays.map((day) => {
                  const at = new Date(day.getFullYear(), day.getMonth(), day.getDate(), hour, 0, 0);
                  const winner = resolveGrid(rules, at);
                  if (winner) {
                    const color = targetColor(winner.target);
                    const tint = /^#[0-9a-f]{6}$/i.test(color) ? `${color}22` : undefined;
                    return (
                      <td
                        key={dateKey(day)}
                        style={{ ...S.cell, background: tint, borderLeft: `2px solid ${color}`, cursor: "pointer" }}
                        title={`${winner.name} · ${t("automation.grid.priority")} ${winner.priority} · ${targetName(winner.target)}`}
                        onClick={() => selectRule(winner.id)}
                      >
                        <div style={S.cellInner}>
                          <span style={{ ...S.cellDot, background: color }} />
                          <span style={S.cellText}>{winner.name}</span>
                        </div>
                      </td>
                    );
                  }
                  return (
                    <td key={dateKey(day)} style={S.cell}
                      title={defaultName ? `${t("automation.grid.defaultClock")}: ${defaultName}` : undefined}>
                      <div style={S.cellInner}>
                        <span style={{ ...S.cellText, color: "var(--text-muted)", opacity: 0.7 }}>
                          {defaultName ?? t("automation.grid.noRule")}
                        </span>
                      </div>
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div style={S.bottom}>
        <div style={S.ruleList}>
          <div style={S.sectionHead}>{t("automation.grid.rules")}</div>
          <div style={S.listScroll}>
            {rules.length === 0 ? (
              <div style={S.empty}>{t("automation.grid.empty")}</div>
            ) : (
              <table style={S.ruleTable}>
                <thead>
                  <tr>
                    <th style={S.th}>{t("automation.grid.name")}</th>
                    <th style={S.th}>{t("automation.grid.priority")}</th>
                    <th style={S.th}>{t("automation.grid.target")}</th>
                    <th style={S.th}>{t("automation.grid.days")}</th>
                    <th style={S.th}>{t("automation.grid.hours")}</th>
                    <th style={S.th}>{t("automation.grid.dates")}</th>
                    <th style={S.th}>{t("automation.grid.enabled")}</th>
                  </tr>
                </thead>
                <tbody>
                  {[...rules]
                    .sort((a, b) => b.priority - a.priority || a.name.localeCompare(b.name))
                    .map((rule) => (
                      <tr
                        key={rule.id}
                        style={{ ...S.ruleRow, ...(selected === rule.id ? S.activeRow : {}) }}
                        onClick={() => selectRule(rule.id)}
                      >
                        <td style={S.td} title={rule.name}>{rule.name}</td>
                        <td style={S.td}>{rule.priority}</td>
                        <td style={S.td} title={targetName(rule.target)}>{targetName(rule.target)}</td>
                        <td style={S.td} title={daysLabel(rule.weekdays)}>{daysLabel(rule.weekdays)}</td>
                        <td style={{ ...S.td, fontFamily: "var(--font-mono)" }}>
                          {rule.start.slice(0, 5)}–{rule.end.slice(0, 5)}
                        </td>
                        <td style={S.td}>{datesLabel(rule)}</td>
                        <td style={S.td}>
                          {rule.enabled ? t("automation.grid.yes") : t("automation.grid.no")}
                        </td>
                      </tr>
                    ))}
                </tbody>
              </table>
            )}
          </div>
        </div>

        <div style={S.editor}>
          {!draft ? (
            <div style={S.empty}>
              <div>{t("automation.grid.selectPrompt")}</div>
              <button type="button" style={{ ...S.btn, marginTop: 10 }} onClick={newRule}>
                {t("automation.grid.new")}
              </button>
            </div>
          ) : (
            <>
              <div style={S.fields}>
                <label style={S.field}>
                  {t("automation.grid.name")}
                  <input
                    value={draft.name}
                    style={{ ...S.input, width: 170 }}
                    onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                  />
                </label>
                <label style={S.field}>
                  {t("automation.grid.priority")}
                  <input
                    type="number"
                    value={draft.priority}
                    style={{ ...S.input, width: 64 }}
                    onChange={(e) => setDraft({ ...draft, priority: Number(e.target.value) })}
                  />
                </label>
                <label style={S.field}>
                  {t("automation.grid.target")}
                  <div style={S.targetRow}>
                    <select
                      value={targetKind}
                      aria-label={t("automation.grid.target")}
                      style={S.input}
                      onChange={(e) => setTargetKind(e.target.value as "Clock" | "Playlist")}
                    >
                      <option value="Clock">{t("automation.grid.targetClock")}</option>
                      <option value="Playlist">{t("automation.grid.targetPlaylist")}</option>
                    </select>
                    <select
                      value={targetId}
                      aria-label={t("automation.grid.target")}
                      style={{ ...S.input, maxWidth: 180 }}
                      onChange={(e) => setTargetId(e.target.value)}
                    >
                      {targetKind === "Clock"
                        ? clocks.map((clock) => (
                          <option key={clock.id} value={clock.id}>{clock.name}</option>
                        ))
                        : playlists.map((playlist) => (
                          <option key={playlist.id} value={playlist.id}>{playlist.name}</option>
                        ))}
                    </select>
                  </div>
                </label>
                <label style={S.field}>
                  {t("automation.grid.start")}
                  <input
                    type="time" step={1}
                    value={draft.start.slice(0, 8)}
                    style={{ ...S.input, width: 100 }}
                    onChange={(e) => setDraft({ ...draft, start: normalizeTime(e.target.value) })}
                  />
                </label>
                <label style={S.field}>
                  {t("automation.grid.end")}
                  <input
                    type="time" step={1}
                    value={draft.end.slice(0, 8)}
                    style={{ ...S.input, width: 100 }}
                    onChange={(e) => setDraft({ ...draft, end: normalizeTime(e.target.value) })}
                  />
                </label>
                <label style={S.field}>
                  {t("automation.grid.dateFrom")}
                  <input
                    type="date"
                    value={draft.date_from ?? ""}
                    style={S.input}
                    onChange={(e) => setDraft({ ...draft, date_from: e.target.value || null })}
                  />
                </label>
                <label style={S.field}>
                  {t("automation.grid.dateTo")}
                  <input
                    type="date"
                    value={draft.date_to ?? ""}
                    style={S.input}
                    onChange={(e) => setDraft({ ...draft, date_to: e.target.value || null })}
                  />
                </label>
                <label style={{ ...S.check, alignSelf: "center" }}>
                  <input
                    type="checkbox"
                    checked={draft.enabled}
                    onChange={(e) => setDraft({ ...draft, enabled: e.target.checked })}
                  />
                  {t("automation.grid.enabled")}
                </label>
              </div>

              <div style={S.group}>
                <div style={S.groupLabel}>{t("automation.grid.days")}</div>
                <div style={S.checks}>
                  {DAYS.map((day, bit) => (
                    <label key={day} style={S.check}>
                      <input
                        type="checkbox"
                        checked={(draft.weekdays & (1 << bit)) !== 0}
                        onChange={(e) => setDraft({
                          ...draft,
                          weekdays: e.target.checked
                            ? draft.weekdays | (1 << bit)
                            : draft.weekdays & ~(1 << bit),
                        })}
                      />
                      {t(`automation.grid.day.${day}`)}
                    </label>
                  ))}
                </div>
              </div>

              <div style={S.group}>
                <div style={S.groupLabel}>{t("automation.grid.months")}</div>
                <div style={S.checks}>
                  {MONTHS.map((month, bit) => (
                    <label key={month} style={S.check}>
                      <input
                        type="checkbox"
                        checked={(draft.months & (1 << bit)) !== 0}
                        onChange={(e) => setDraft({
                          ...draft,
                          months: e.target.checked
                            ? draft.months | (1 << bit)
                            : draft.months & ~(1 << bit),
                        })}
                      />
                      {t(`automation.grid.month.${month}`)}
                    </label>
                  ))}
                </div>
              </div>

              <div style={S.group}>
                <div style={S.groupLabel}>{t("automation.grid.weeks")}</div>
                <div style={S.checks}>
                  {WEEKS.map((week, bit) => (
                    <label key={week} style={S.check}>
                      <input
                        type="checkbox"
                        checked={(draft.weeks_of_month & (1 << bit)) !== 0}
                        onChange={(e) => setDraft({
                          ...draft,
                          weeks_of_month: e.target.checked
                            ? draft.weeks_of_month | (1 << bit)
                            : draft.weeks_of_month & ~(1 << bit),
                        })}
                      />
                      {t(`automation.grid.${week}`)}
                    </label>
                  ))}
                </div>
              </div>

              <div style={S.hint}>{t("automation.grid.hint")}</div>

              <div style={S.actions}>
                <button type="button" style={S.btnPrimary} onClick={save}>
                  {t("automation.grid.save")}
                </button>
                <button type="button" style={S.btn} onClick={removeRule}>
                  {t("automation.grid.delete")}
                </button>
                <button type="button" style={S.btn} onClick={newRule}>
                  {t("automation.grid.new")}
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
