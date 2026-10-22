import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

interface EngineState {
  state: "Stopped" | "Playing" | "Paused";
  volume: number;
  current_track: string | null;
  position_ms: number;
  duration_ms: number;
}

const S: Record<string, React.CSSProperties> = {
  wrap: { padding: "16px 12px", borderBottom: "1px solid var(--border)" },
  state: { display: "flex", alignItems: "center", gap: 6, fontSize: 11, fontWeight: 700, letterSpacing: 1, marginBottom: 10 },
  dot: { width: 8, height: 8, borderRadius: "50%", display: "inline-block" },
  track: { fontSize: 15, fontWeight: 600, marginBottom: 12, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  bar: { height: 4, background: "var(--bg-input)", borderRadius: 2, overflow: "hidden", marginBottom: 6 },
  fill: { height: "100%", background: "var(--accent)", borderRadius: 2 },
  times: { display: "flex", justifyContent: "space-between", fontSize: 11, fontFamily: "var(--font-mono)", color: "var(--text-muted)" },
};

export default function NowPlaying() {
  const { t } = useTranslation();
  const [es, setEs] = useState<EngineState>({
    state: "Stopped", volume: 0.8, current_track: null, position_ms: 0, duration_ms: 0,
  });

  useEffect(() => {
    const id = setInterval(async () => {
      try { setEs(await invoke<EngineState>("engine_get_state")); } catch { /**/ }
    }, 250);
    return () => clearInterval(id);
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
      </div>
      <div style={S.track}>{es.current_track ?? t("nowPlaying.noTrack")}</div>
      <div style={S.bar}><div style={{ ...S.fill, width: `${pct}%`, transition: "width 0.25s linear" }} /></div>
      <div style={S.times}>
        <span>{fmt(es.position_ms)}</span>
        <span>-{fmt(es.duration_ms - es.position_ms)}</span>
        <span>{fmt(es.duration_ms)}</span>
      </div>
    </div>
  );
}
