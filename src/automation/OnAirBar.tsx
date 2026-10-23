import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { KIND_COLORS, fmtDuration, type PlaylistItem } from "./api";
import { useOnAir } from "./useOnAir";

const S: Record<string, React.CSSProperties> = {
  bar: {
    display: "grid", gridTemplateColumns: "150px 1fr 1.3fr 1fr", gap: 6, padding: "6px 8px",
    background: "var(--bg-secondary)", borderBottom: "1px solid var(--border)",
  },
  clock: {
    background: "var(--bg-panel)", borderRadius: "var(--radius)", padding: "4px 8px",
    display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
  },
  date: { fontSize: 11, color: "var(--text-secondary)", textTransform: "capitalize" },
  time: { fontSize: 22, fontWeight: 700, fontFamily: "var(--font-mono)", lineHeight: 1.1 },
  left: { fontSize: 13, fontFamily: "var(--font-mono)", color: "var(--green)" },
  box: {
    borderRadius: "var(--radius)", padding: "4px 10px", minWidth: 0,
    display: "flex", flexDirection: "column", justifyContent: "center",
  },
  label: { fontSize: 10, fontWeight: 700, letterSpacing: 0.6, textTransform: "uppercase", opacity: 0.75 },
  title: { fontSize: 14, fontWeight: 600, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  sub: { fontSize: 11, opacity: 0.8, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  progress: { height: 4, background: "rgba(0,0,0,0.25)", borderRadius: 2, marginTop: 4, overflow: "hidden" },
  times: { display: "flex", justifyContent: "space-between", fontSize: 10, fontFamily: "var(--font-mono)", opacity: 0.8 },
};

// RadioBOSS-like colours: previous orange, on air green, next yellow (dark-theme tints).
const BOX = {
  prev: { background: "#3a2a1a", color: "#f0b27a", border: "1px solid #6b4423" },
  air: { background: "#14321f", color: "#7ee2a0", border: "1px solid #2e7d4a" },
  next: { background: "#33301a", color: "#f2e394", border: "1px solid #6b6123" },
};

function Item({ label, item, style, children }: {
  label: string; item?: PlaylistItem; style: React.CSSProperties; children?: React.ReactNode;
}) {
  const { t } = useTranslation();
  const kind = item?.kind ?? "Music";
  return (
    <div style={{ ...S.box, ...style }} aria-label={label}>
      <span style={S.label}>
        {label}
        {item && kind !== "Music" && (
          <span style={{ marginLeft: 6, color: KIND_COLORS[kind] }}>· {t(`automation.kinds.${kind}`)}</span>
        )}
      </span>
      <span style={S.title} title={item?.track_path}>{item ? item.title : "—"}</span>
      <span style={S.sub}>{item?.artist || " "}</span>
      {children}
    </div>
  );
}

/** Clock, previous track, track on air and next track, as in RadioBOSS. */
export default function OnAirBar() {
  const { t, i18n } = useTranslation();
  const { engine, playlist } = useOnAir();
  const [now, setNow] = useState(new Date());

  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(id);
  }, []);

  const { items, current_index: i } = playlist;
  // Stopped, the current item is the one that would start: show it as next.
  const stopped = engine.state === "Stopped";
  const onAir = stopped ? undefined : items[i];
  const previous = items[i - 1];
  const next = stopped ? items[i] : items[i + 1];
  const remaining = Math.max(0, engine.duration_ms - engine.position_ms);
  const pct = engine.duration_ms > 0 ? (engine.position_ms / engine.duration_ms) * 100 : 0;

  return (
    <div style={S.bar}>
      <div style={S.clock}>
        <span style={S.date}>
          {now.toLocaleDateString(i18n.language, { weekday: "short", day: "2-digit", month: "2-digit", year: "numeric" })}
        </span>
        <span style={S.time}>{now.toLocaleTimeString(i18n.language, { hour12: false })}</span>
        <span style={S.left} title={t("automation.onAir.remaining")}>
          {engine.state !== "Stopped" ? `-${fmtDuration(remaining)}` : " "}
        </span>
      </div>
      <Item label={t("automation.onAir.previous")} item={previous} style={BOX.prev} />
      <Item label={engine.state === "Paused" ? t("nowPlaying.paused") : t("automation.onAir.onAir")} item={onAir} style={BOX.air}>
        <div style={S.progress}><div style={{ width: `${pct}%`, height: "100%", background: "currentColor" }} /></div>
        <div style={S.times}>
          <span>{fmtDuration(engine.position_ms)}</span>
          <span>{fmtDuration(engine.duration_ms)}</span>
        </div>
      </Item>
      <Item label={t("automation.onAir.next")} item={next} style={BOX.next} />
    </div>
  );
}
