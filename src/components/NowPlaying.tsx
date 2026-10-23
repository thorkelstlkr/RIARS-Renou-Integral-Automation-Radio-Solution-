import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface EngineState {
  state: "Stopped" | "Playing" | "Paused";
  volume: number;
  current_track: string | null;
  position_ms: number;
  duration_ms: number;
  dead_air?: boolean;
  dead_air_count?: number;
}

const S: Record<string, React.CSSProperties> = {
  wrap: { padding: "16px 12px", borderBottom: "1px solid var(--border)" },
  state: { display: "flex", alignItems: "center", gap: 6, fontSize: 11, fontWeight: 700, letterSpacing: 1, marginBottom: 10 },
  dot: { width: 8, height: 8, borderRadius: "50%", display: "inline-block" },
  track: { fontSize: 15, fontWeight: 600, marginBottom: 12, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  bar: { height: 8, background: "var(--bg-input)", borderRadius: 4, overflow: "hidden", marginBottom: 6, cursor: "pointer" },
  fill: { height: "100%", background: "var(--accent)", borderRadius: 2 },
  times: { display: "flex", justifyContent: "space-between", fontSize: 11, fontFamily: "var(--font-mono)", color: "var(--text-muted)" },
};

export default function NowPlaying() {
  const { t } = useTranslation();
  const [es, setEs] = useState<EngineState>({
    state: "Stopped", volume: 0.8, current_track: null, position_ms: 0, duration_ms: 0,
  });

  // One read on mount, then the backend pushes changes.
  useEffect(() => {
    invoke<EngineState>("engine_get_state").then(setEs).catch(() => {});
    const unlisten = listen<EngineState>("engine-state", (e) => setEs(e.payload));
    return () => { unlisten.then((f) => f()); };
  }, []);

  const fmt = (ms: number) => {
    const s = Math.floor(ms / 1000);
    return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
  };

  const pct = es.duration_ms > 0 ? (es.position_ms / es.duration_ms) * 100 : 0;
  const label = es.state === "Playing" ? t("nowPlaying.onAir") : es.state === "Paused" ? t("nowPlaying.paused") : t("nowPlaying.stopped");
  const color = es.state === "Playing" ? "var(--green)" : es.state === "Paused" ? "var(--yellow)" : "var(--text-muted)";

  return (
    <div style={S.wrap}>
      <div style={{ ...S.state, color }}>
        <span style={{ ...S.dot, background: color }} />
        {label}
        {es.dead_air && (
          <span data-testid="dead-air" title={t("nowPlaying.deadAirHint", { count: es.dead_air_count ?? 0 })}
            style={{ marginLeft: 8, padding: "1px 6px", borderRadius: 3, background: "var(--red)", color: "#fff" }}>
            {t("nowPlaying.deadAir")}
          </span>
        )}
      </div>
      <div style={S.track}>{es.current_track ?? t("nowPlaying.noTrack")}</div>
      <div style={S.bar} data-testid="progress" title={t("nowPlaying.seekHint")}
        onClick={(e) => {
          if (es.duration_ms <= 0) return;
          const r = e.currentTarget.getBoundingClientRect();
          const fraction = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
          const position = Math.round(fraction * es.duration_ms);
          setEs((s) => ({ ...s, position_ms: position })); // move the bar now, not on the next poll
          invoke("engine_seek", { positionMs: position }).catch(() => {});
        }}>
        <div style={{ ...S.fill, width: `${pct}%`, transition: "width 0.25s linear" }} />
      </div>
      <div style={S.times}>
        <span>{fmt(es.position_ms)}</span>
        <span>-{fmt(es.duration_ms - es.position_ms)}</span>
        <span>{fmt(es.duration_ms)}</span>
      </div>
    </div>
  );
}
