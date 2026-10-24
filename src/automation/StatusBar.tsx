import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { automation, AUTOMATION_EVENT } from "./api";
import type { EngineState } from "./useOnAir";

const S: Record<string, React.CSSProperties> = {
  bar: {
    display: "flex", alignItems: "center", gap: 0, background: "var(--bg-secondary)",
    borderTop: "1px solid var(--border)", fontSize: 12, color: "var(--text-secondary)", minHeight: 26,
  },
  cell: { display: "flex", alignItems: "center", gap: 6, padding: "0 12px", borderRight: "1px solid var(--border)", height: 26 },
  dot: { width: 8, height: 8, borderRadius: "50%", display: "inline-block" },
  grow: { flex: 1 },
};

function Cell({ ok, label, alarm }: { ok: boolean; label: string; alarm?: boolean }) {
  const color = alarm ? "var(--red)" : ok ? "var(--green)" : "var(--text-muted)";
  return (
    <span style={S.cell}>
      <span style={{ ...S.dot, background: color }} aria-hidden />
      <span style={alarm ? { color: "var(--red)", fontWeight: 600 } : undefined}>{label}</span>
    </span>
  );
}

/** Bottom bar: silence detector, automation, event scheduler, version. */
export default function StatusBar() {
  const { t } = useTranslation();
  const [deadAirOn, setDeadAirOn] = useState(false);
  const [silent, setSilent] = useState(false);
  const [auto, setAuto] = useState(false);
  const [events, setEvents] = useState(0);

  const load = useCallback(async () => {
    try {
      const [deadAir, status, schedule] = await Promise.all([
        invoke<{ enabled: boolean }>("dead_air_get"),
        automation.status(),
        invoke<{ enabled: boolean }[]>("schedule_get"),
      ]);
      setDeadAirOn(deadAir.enabled);
      setAuto(status.enabled);
      setEvents(schedule.filter((e) => e.enabled).length);
    } catch { /* backend not ready */ }
  }, []);

  useEffect(() => {
    load();
    const offState = listen<EngineState>("engine-state", (e) => setSilent(!!e.payload.dead_air));
    const offAuto = listen(AUTOMATION_EVENT, () => load());
    // Settings change from other panels: a slow refresh is enough for a status bar.
    const id = setInterval(load, 15_000);
    return () => { offState.then((f) => f()); offAuto.then((f) => f()); clearInterval(id); };
  }, [load]);

  return (
    <footer style={S.bar}>
      <Cell ok={deadAirOn} alarm={silent}
        label={silent ? t("automation.status.silence") : deadAirOn ? t("automation.status.deadAirOn") : t("automation.status.deadAirOff")} />
      <Cell ok={auto} label={auto ? t("automation.status.autoOn") : t("automation.status.autoOff")} />
      <Cell ok={events > 0} label={t("automation.status.events", { count: events })} />
      <span style={S.grow} />
      <span style={{ ...S.cell, borderRight: "none", color: "var(--green)" }}>{t("app.version")}</span>
    </footer>
  );
}
