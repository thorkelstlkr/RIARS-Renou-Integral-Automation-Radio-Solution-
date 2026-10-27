import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { analysis } from "../analysis/api";
import { fmtDuration } from "../automation/api";
import { useOnAir } from "../automation/useOnAir";
import CartGrid from "./CartGrid";
import MicSettingsDialog from "./MicSettingsDialog";
import { live, type LiveStatus, type PlayMode } from "./api";

const S: Record<string, React.CSSProperties> = {
  panel: { display: "flex", flexDirection: "column" as const, gap: 16, height: "100%", padding: 16, overflowY: "auto" as const },
  micStrip: {
    display: "flex", alignItems: "center", gap: 14, padding: 12,
    background: "var(--bg-secondary)", border: "1px solid var(--border)", borderRadius: "var(--radius)",
  },
  mic: {
    display: "flex", flexDirection: "column" as const, alignItems: "center", justifyContent: "center",
    width: 92, height: 72, borderRadius: "var(--radius)", border: "2px solid var(--border)",
    background: "var(--bg-panel)", color: "var(--text-primary)", fontSize: 20, fontWeight: 800,
    letterSpacing: 1, flexShrink: 0,
  },
  micOn: { background: "var(--red)", borderColor: "var(--red)", color: "#fff" },
  shortcut: { fontSize: 10, fontWeight: 600, opacity: 0.7, marginTop: 2, letterSpacing: 0 },
  meterBox: { flex: 1, minWidth: 0, display: "flex", flexDirection: "column" as const, gap: 6 },
  meterRow: { display: "flex", alignItems: "center", gap: 10 },
  meter: {
    position: "relative" as const, flex: 1, height: 18, borderRadius: 4, overflow: "hidden",
    background: "var(--bg-panel)", border: "1px solid var(--border)",
  },
  meterFill: { height: "100%", transition: "width 120ms linear" },
  peak: { position: "absolute" as const, top: 0, bottom: 0, width: 2, background: "var(--text-primary)" },
  labels: { display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" as const, fontSize: 12 },
  onAir: { display: "flex", alignItems: "center", gap: 6, color: "var(--red)", fontWeight: 800, letterSpacing: 1 },
  dot: { fontSize: 14 },
  speaking: { display: "flex", alignItems: "center", gap: 5, color: "var(--green)" },
  speakingDot: { width: 8, height: 8, borderRadius: "50%", background: "var(--green)", display: "inline-block" },
  device: { fontSize: 11, color: "var(--text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  error: { fontSize: 11, color: "var(--red)", wordBreak: "break-word" as const },
  gear: { padding: "6px 10px", borderRadius: "var(--radius)", background: "var(--bg-panel)", fontSize: 16, flexShrink: 0 },
  section: { display: "flex", flexDirection: "column" as const, gap: 6 },
  sectionTitle: {
    fontSize: 12, fontWeight: 700, letterSpacing: 0.6, textTransform: "uppercase" as const,
    color: "var(--text-secondary)",
  },
  segment: { display: "flex", gap: 4 },
  segmentBtn: {
    padding: "6px 14px", borderRadius: "var(--radius)", fontSize: 13,
    background: "var(--bg-panel)", color: "var(--text-secondary)",
  },
  segmentOn: { background: "var(--accent)", color: "#fff" },
  hint: { fontSize: 12, color: "var(--text-muted)" },
  info: { display: "flex", flexDirection: "column" as const, gap: 4, padding: 12, background: "var(--bg-secondary)", border: "1px solid var(--border)", borderRadius: "var(--radius)" },
  current: { fontSize: 18, fontWeight: 700, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  times: { display: "flex", alignItems: "baseline", gap: 14, flexWrap: "wrap" as const },
  time: { fontFamily: "var(--font-mono)", fontSize: 14, color: "var(--text-secondary)" },
  intro: { fontSize: 30, fontWeight: 800, fontFamily: "var(--font-mono)", lineHeight: 1 },
  next: { fontSize: 12, color: "var(--text-secondary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  notice: { fontSize: 12, color: "var(--red)", wordBreak: "break-word" as const },
};

const dbOf = (level: number) => (level <= 0 ? -60 : Math.max(-60, 20 * Math.log10(level)));
const levelColor = (db: number) => (db < -12 ? "var(--green)" : db < -3 ? "var(--yellow)" : "var(--red)");

const MODES: PlayMode[] = ["Automatic", "Assisted", "Manual"];

/** The On air tab: microphone, play mode, what is on air and the sound bank. */
export default function LivePanel() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<LiveStatus | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [settings, setSettings] = useState(false);
  const [introEnd, setIntroEnd] = useState<number | null>(null);
  const [peakDb, setPeakDb] = useState(-60);
  const peak = useRef({ db: -60, at: 0 });
  const { engine, playlist } = useOnAir();

  // Poll while the tab is shown, and open the input card (off air) so the meter works.
  useEffect(() => {
    let alive = true;
    let timer: number | undefined;
    const tick = async () => {
      try {
        const next = await live.status();
        if (!alive) return;
        setStatus(next);
        setStatusError(null);
      } catch (e) {
        if (alive) setStatusError(String(e));
      } finally {
        if (alive) timer = window.setTimeout(tick, 150);
      }
    };
    void tick();
    live.micOpen().catch((e) => { if (alive) setNotice(String(e)); });
    return () => {
      alive = false;
      if (timer !== undefined) clearTimeout(timer);
    };
  }, []);

  const level = status?.mic.level ?? 0;
  const db = dbOf(level);
  const percent = Math.max(0, Math.min(100, ((db + 60) / 60) * 100));
  const peakPercent = Math.max(0, Math.min(100, ((peakDb + 60) / 60) * 100));

  // Peak hold for one second, then fall with the level.
  useEffect(() => {
    if (!status) return;
    const now = Date.now();
    const next = dbOf(status.mic.level);
    if (next >= peak.current.db || now - peak.current.at > 1000) {
      peak.current = { db: next, at: now };
      setPeakDb(next);
    }
  }, [status]);

  const micOn = status?.mic.on ?? false;
  const mode = status?.mode ?? "Automatic";
  const onAirItem = playlist.items[playlist.current_index];
  const nextItem = playlist.items[playlist.current_index + 1];
  const onAirPath = onAirItem?.track_path;

  useEffect(() => {
    if (!onAirPath) {
      setIntroEnd(null);
      return;
    }
    let alive = true;
    analysis.pathsCues([onAirPath])
      .then((list) => {
        if (!alive) return;
        const found = list.find((item) => item.path === onAirPath);
        setIntroEnd(found?.cues.intro_end_ms ?? null);
      })
      .catch(() => { if (alive) setIntroEnd(null); });
    return () => { alive = false; };
  }, [onAirPath]);

  const toggleMic = async () => {
    try {
      await live.setMic(!micOn);
      setNotice(null);
    } catch (e) {
      setNotice(String(e));
    }
  };

  const setMode = async (next: PlayMode) => {
    try {
      await live.setMode(next);
      setStatus((prev) => (prev ? { ...prev, mode: next } : prev));
      setNotice(null);
    } catch (e) {
      setNotice(String(e));
    }
  };

  const duration = onAirItem?.duration_ms || engine.duration_ms;
  const leftMs = onAirItem ? Math.max(0, duration - engine.position_ms) : 0;
  const introLeft = introEnd != null && engine.position_ms < introEnd
    ? Math.ceil((introEnd - engine.position_ms) / 1000)
    : null;

  return (
    <div style={S.panel}>
      <style>{`@keyframes riars-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.2; } }`}</style>

      <div style={S.micStrip}>
        <button style={{ ...S.mic, ...(micOn ? S.micOn : {}) }} onClick={() => { void toggleMic(); }}
          aria-pressed={micOn} title={`${t("live.mic")} (F9)`}>
          {t("live.mic")}
          <span style={S.shortcut}>F9</span>
        </button>

        <div style={S.meterBox}>
          <div style={S.meterRow}>
            <div style={S.meter} role="meter" aria-label={t("live.mic")}
              aria-valuemin={-60} aria-valuemax={0} aria-valuenow={Math.round(db)}>
              <div style={{ ...S.meterFill, width: `${percent}%`, background: levelColor(db) }} />
              <div style={{ ...S.peak, left: `calc(${peakPercent}% - 1px)` }} />
            </div>
            {micOn && (
              <span style={S.onAir}>
                <span style={{ ...S.dot, animation: "riars-pulse 1s infinite" }}>●</span>
                {t("live.onAir")}
              </span>
            )}
            {status?.mic.speaking && (
              <span style={S.speaking}><span style={S.speakingDot} />{t("live.speaking")}</span>
            )}
          </div>
          {status?.mic.error
            ? <div style={S.error}>{status.mic.error}</div>
            : <div style={S.device}>{status?.mic.device ?? ""}</div>}
          {(notice || statusError) && <div style={S.error}>{notice ?? statusError}</div>}
        </div>

        <button style={S.gear} onClick={() => setSettings(true)}
          title={t("live.micSettings")} aria-label={t("live.micSettings")}>⚙</button>
      </div>

      <div style={S.section}>
        <span style={S.sectionTitle}>{t("live.playMode")}</span>
        <div style={S.segment} role="group" aria-label={t("live.playMode")}>
          {MODES.map((item) => (
            <button key={item} style={{ ...S.segmentBtn, ...(mode === item ? S.segmentOn : {}) }}
              aria-pressed={mode === item} onClick={() => { void setMode(item); }}>
              {t(`live.mode.${item}`)}
            </button>
          ))}
        </div>
        <div style={S.hint}>{t(`live.modeHint.${mode}`)}</div>
      </div>

      <div style={S.info}>
        <div style={S.current}>{onAirItem?.title ?? "—"}</div>
        <div style={S.times}>
          <span style={S.time}>{onAirItem ? fmtDuration(leftMs) : "—"}</span>
          {introLeft != null && (
            <span style={{ ...S.intro, color: introLeft <= 3 ? "var(--yellow)" : "var(--green)" }}>
              {t("live.introLeft", { s: introLeft })}
            </span>
          )}
        </div>
        {nextItem && <div style={S.next}>{t("live.next", { title: nextItem.title })}</div>}
      </div>

      <div style={S.section}>
        <span style={S.sectionTitle}>{t("live.bank")}</span>
        <CartGrid carts={status?.carts ?? []} />
      </div>

      {settings && <MicSettingsDialog onClose={() => setSettings(false)} />}
    </div>
  );
}
