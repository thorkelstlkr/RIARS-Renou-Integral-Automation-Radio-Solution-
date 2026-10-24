import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { automation, AUTOMATION_EVENT, timeOf, type AutomationEvent, type AutomationStatus } from "./api";

const S: Record<string, React.CSSProperties> = {
  wrap: { padding: "12px", borderBottom: "1px solid var(--border)", fontSize: 12 },
  head: { display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 8 },
  title: { fontSize: 11, fontWeight: 700, letterSpacing: 1, textTransform: "uppercase", color: "var(--text-secondary)" },
  toggle: { padding: "3px 10px", borderRadius: 12, fontSize: 11, fontWeight: 700 },
  row: { display: "flex", justifyContent: "space-between", gap: 8, padding: "2px 0" },
  key: { color: "var(--text-muted)" },
  val: { fontFamily: "var(--font-mono)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  warn: { color: "var(--yellow)", marginTop: 6, fontSize: 11, lineHeight: 1.4 },
  error: { color: "var(--red)", marginTop: 6, fontSize: 11 },
};

/** Automation on/off and what it is doing now. */
export default function AutomationCard() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<AutomationStatus | null>(null);
  const [names, setNames] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [s, clocks, grid] = await Promise.all([automation.status(), automation.clocks(), automation.grid()]);
      setStatus(s);
      // Names to show instead of ids (clock and rule ids do not collide in practice).
      setNames(Object.fromEntries([
        ...clocks.map((c) => [`clock:${c.id}`, c.name]),
        ...grid.map((r) => [`rule:${r.id}`, r.name || r.id]),
      ]));
      setError(null);
    } catch (e) { setError(String(e)); }
  }, []);

  useEffect(() => {
    load();
    // Changes come with automation events; the period also moves with the clock.
    const off = listen<AutomationEvent>(AUTOMATION_EVENT, () => load());
    const id = setInterval(load, 30_000);
    return () => { off.then((f) => f()); clearInterval(id); };
  }, [load]);

  const toggle = async () => {
    if (!status) return;
    try {
      await automation.setEnabled(!status.enabled);
      // The generator takes the change on its next tick (a quarter of a second).
      setTimeout(load, 600);
    } catch (e) { setError(String(e)); }
  };

  const on = status?.enabled ?? false;
  const period = status?.period;
  const last = status?.warnings[status.warnings.length - 1];
  return (
    <div style={S.wrap}>
      <div style={S.head}>
        <span style={S.title}>{t("automation.card.title")}</span>
        <button onClick={toggle} aria-pressed={on}
          style={{ ...S.toggle, background: on ? "var(--green)" : "var(--bg-input)", color: on ? "#0d1117" : "var(--text-secondary)" }}>
          {on ? t("automation.card.on") : t("automation.card.off")}
        </button>
      </div>
      {period && (
        <>
          <div style={S.row}>
            <span style={S.key}>{period.clock_id ? t("automation.card.clock") : t("automation.card.playlist")}</span>
            <span style={S.val}>{period.clock_id ? names[`clock:${period.clock_id}`] ?? period.clock_id : period.playlist_id}</span>
          </div>
          <div style={S.row}>
            <span style={S.key}>{t("automation.card.period")}</span>
            <span style={S.val}>{timeOf(period.start).slice(0, 5)}–{timeOf(period.end).slice(0, 5)}</span>
          </div>
          {period.rule_id && (
            <div style={S.row}>
              <span style={S.key}>{t("automation.card.rule")}</span>
              <span style={S.val}>{names[`rule:${period.rule_id}`] ?? period.rule_id}</span>
            </div>
          )}
        </>
      )}
      {on && !period && <div style={S.key}>{t("automation.card.starting")}</div>}
      {last && (
        <div style={S.warn} title={status!.warnings.map(([at, w]) => `${timeOf(at)} ${w}`).join("\n")}>
          ⚠ {timeOf(last[0]).slice(0, 5)} {last[1]}
          {status!.warnings.length > 1 && ` (+${status!.warnings.length - 1})`}
        </div>
      )}
      {error && <div style={S.error}>{error}</div>}
    </div>
  );
}
