import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import Dialog from "../menu/Dialog";
import Waveform, { type WaveformMark, type WaveformMarkId } from "./Waveform";
import {
  analysis, audio, preview,
  type AudioDevices, type CueAnalysis, type CueSource, type NormalizationConfig, type TrackCues,
} from "./api";

interface MarkValues {
  cueIn: number;
  cueOut: number;
  introEnd: number | null;
  outroStart: number | null;
}

interface Meta {
  source: CueSource;
  confidence: number;
  loudness: number | null;
  peak: number | null;
  analyzedAt: string | null;
}

const S: Record<string, React.CSSProperties> = {
  top: { display: "flex", alignItems: "center", gap: 10, marginBottom: 8, fontSize: 12, color: "var(--text-secondary)" },
  spinner: { fontSize: 12, color: "var(--yellow)" },
  wave: { border: "1px solid var(--border)", borderRadius: "var(--radius)", marginBottom: 6 },
  stripWrap: { marginBottom: 10 },
  stripLabel: { fontSize: 10, color: "var(--text-muted)", marginBottom: 3, textTransform: "uppercase", letterSpacing: 0.5 },
  strip: {
    display: "flex", height: 12, border: "1px solid var(--border)", borderRadius: 2,
    overflow: "hidden", background: "var(--bg-input)",
  },
  fields: { display: "grid", gridTemplateColumns: "repeat(4, minmax(100px, 1fr))", gap: 8, marginBottom: 6 },
  field: { display: "flex", flexDirection: "column", gap: 2, fontSize: 11, color: "var(--text-secondary)" },
  input: { fontSize: 13, width: "100%" },
  checks: { display: "flex", gap: 18, marginBottom: 8 },
  check: { display: "flex", alignItems: "center", gap: 6, fontSize: 12 },
  info: {
    display: "flex", flexWrap: "wrap" as const, gap: "4px 16px", fontSize: 12,
    color: "var(--text-secondary)", borderTop: "1px solid var(--border)", paddingTop: 8, marginTop: 4,
  },
  room: { color: "var(--text-secondary)" },
  prelisten: {
    display: "flex", flexWrap: "wrap" as const, alignItems: "center", gap: 8,
    marginTop: 10, paddingTop: 10, borderTop: "1px solid var(--border)",
  },
  subtitle: {
    fontSize: 11, fontWeight: 700, textTransform: "uppercase" as const, letterSpacing: 0.5,
    color: "var(--text-muted)", marginRight: 4,
  },
  btn: { padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12, background: "var(--bg-panel)" },
  btnPrimary: { padding: "4px 12px", borderRadius: "var(--radius)", fontSize: 12, background: "var(--accent)", color: "#fff" },
  hint: { fontSize: 11, color: "var(--text-muted)" },
  error: { fontSize: 12, color: "var(--red)", marginTop: 6, wordBreak: "break-word" as const },
  notice: { fontSize: 12, color: "var(--green)", marginTop: 6 },
  actions: { display: "flex", flexWrap: "wrap" as const, gap: 8, marginTop: 12, paddingTop: 10, borderTop: "1px solid var(--border)" },
  spacer: { flex: 1 },
};

const toFields = (marks: MarkValues) => ({
  cueIn: toField(marks.cueIn),
  cueOut: toField(marks.cueOut),
  introEnd: toField(marks.introEnd),
  outroStart: toField(marks.outroStart),
});

const toField = (ms: number | null) => {
  if (ms == null) return "";
  const total = Math.max(0, ms) / 1000;
  const m = Math.floor(total / 60);
  const s = total - m * 60;
  return `${m}:${s.toFixed(1).padStart(4, "0")}`;
};

const parseField = (value: string): number | null => {
  const text = value.trim();
  if (text === "") return null;
  const parts = text.split(":");
  if (parts.length > 2) return null;
  const minutes = parts.length === 2 ? Number(parts[0]) : 0;
  const seconds = Number(parts[parts.length - 1]);
  if (!Number.isFinite(minutes) || !Number.isFinite(seconds) || minutes < 0 || seconds < 0) return null;
  return Math.round((minutes * 60 + seconds) * 1000);
};

const fromCues = (cues: TrackCues): MarkValues => ({
  cueIn: cues.cue_in_ms,
  cueOut: cues.cue_out_ms,
  introEnd: cues.intro_end_ms,
  outroStart: cues.outro_start_ms,
});

const fromAnalysis = (result: CueAnalysis): MarkValues => ({
  cueIn: result.cue_in_ms,
  cueOut: result.cue_out_ms,
  introEnd: result.intro_ms,
  outroStart: result.outro_ms,
});

const metaFromCues = (cues: TrackCues): Meta => ({
  source: cues.source,
  confidence: cues.confidence,
  loudness: cues.loudness_lufs,
  peak: cues.peak_dbfs,
  analyzedAt: cues.analyzed_at,
});

const metaFromAnalysis = (result: CueAnalysis): Meta => ({
  source: "Auto",
  confidence: result.confidence,
  loudness: result.loudness_lufs,
  peak: result.peak_dbfs,
  // The backend stamps the time of a save; a browser clock in UTC is not wanted.
  analyzedAt: null,
});

const gainDb = (config: NormalizationConfig, loudness: number | null, peak: number | null) => {
  if (!config.enabled || loudness == null) return 0;
  let gain = Math.max(-config.max_cut_db, Math.min(config.max_gain_db, config.target_lufs - loudness));
  if (gain > 0 && peak != null) gain = Math.min(gain, Math.max(0, config.peak_ceiling_dbfs - peak));
  return gain;
};

const numberText = (value: number | null, digits = 1) => (value == null ? "—" : value.toFixed(digits));
const signed = (value: number) => `${value >= 0 ? "+" : ""}${value.toFixed(1)}`;

const sameMarks = (a: MarkValues, b: MarkValues) =>
  a.cueIn === b.cueIn && a.cueOut === b.cueOut && a.introEnd === b.introEnd && a.outroStart === b.outroStart;

/** Cue-point editor with waveform, loudness info and pre-listening. */
export default function CueEditor({ trackId, title, onClose }: {
  trackId: number; title: string; onClose: () => void;
}) {
  const { t } = useTranslation();
  const [cues, setCues] = useState<TrackCues | null>(null);
  const [analysisData, setAnalysisData] = useState<CueAnalysis | null>(null);
  const [analysing, setAnalysing] = useState(true);
  const [draft, setDraft] = useState<MarkValues | null>(null);
  const [fields, setFields] = useState(() => toFields({ cueIn: 0, cueOut: 0, introEnd: null, outroStart: null }));
  const [saved, setSaved] = useState<MarkValues | null>(null);
  const [meta, setMeta] = useState<Meta | null>(null);
  const [normalization, setNormalization] = useState<NormalizationConfig | null>(null);
  const [devices, setDevices] = useState<AudioDevices | null>(null);
  const [playing, setPlaying] = useState(false);
  const [positionMs, setPositionMs] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const cuesRef = useRef<TrackCues | null>(null);
  const cuesLoaded = useRef(false);
  const analysisRef = useRef<CueAnalysis | null>(null);
  const savedRef = useRef<MarkValues | null>(null);
  savedRef.current = saved;

  const showDraft = useCallback((marks: MarkValues) => {
    setDraft(marks);
    setFields(toFields(marks));
  }, []);

  // Stored marks first; a track without them starts from the fresh analysis.
  useEffect(() => {
    let alive = true;
    cuesLoaded.current = false;
    cuesRef.current = null;
    analysisRef.current = null;
    setAnalysing(true);
    setError(null);

    analysis.trackCues(trackId)
      .then((stored) => {
        if (!alive) return;
        cuesLoaded.current = true;
        cuesRef.current = stored;
        setCues(stored);
        if (stored) {
          const marks = fromCues(stored);
          showDraft(marks);
          setSaved(marks);
          setMeta(metaFromCues(stored));
        } else if (analysisRef.current) {
          const marks = fromAnalysis(analysisRef.current);
          showDraft(marks);
          setSaved(marks);
          setMeta(metaFromAnalysis(analysisRef.current));
        }
      })
      .catch((e) => {
        if (!alive) return;
        setError(String(e));
        // Treat a failed lookup as "no stored marks" so the analysis can still seed the editor.
        cuesLoaded.current = true;
        if (analysisRef.current && cuesRef.current == null) {
          const marks = fromAnalysis(analysisRef.current);
          showDraft(marks);
          setSaved(marks);
          setMeta(metaFromAnalysis(analysisRef.current));
        }
      });

    analysis.analyzeTrack(trackId, false)
      .then((result) => {
        if (!alive) return;
        analysisRef.current = result;
        setAnalysisData(result);
        if (cuesLoaded.current && cuesRef.current == null) {
          const marks = fromAnalysis(result);
          showDraft(marks);
          setSaved(marks);
          setMeta(metaFromAnalysis(result));
        }
      })
      .catch((e) => { if (alive) setError(String(e)); })
      .finally(() => { if (alive) setAnalysing(false); });

    return () => { alive = false; };
  }, [trackId, showDraft]);

  useEffect(() => {
    let alive = true;
    audio.devices().then((list) => { if (alive) setDevices(list); }).catch(() => {});
    audio.getNormalization().then((config) => { if (alive) setNormalization(config); }).catch(() => {});
    return () => { alive = false; };
  }, []);

  // The play-head follows the pre-listen engine only while it is playing.
  useEffect(() => {
    if (!playing) return;
    let alive = true;
    const id = setInterval(async () => {
      try {
        const state = await preview.state();
        if (!alive) return;
        setPositionMs(state.playing ? state.position_ms : null);
        if (!state.playing) setPlaying(false);
      } catch { /* the card may have gone */ }
    }, 250);
    return () => { alive = false; clearInterval(id); };
  }, [playing]);

  const durationMs = cues?.duration_ms ?? analysisData?.duration_ms ?? 0;

  const setMark = (id: WaveformMarkId, ms: number) => {
    const value = Math.max(0, Math.min(durationMs, Math.round(ms)));
    setDraft((current) => {
      if (!current) return current;
      if (id === "cueIn") return { ...current, cueIn: value };
      if (id === "cueOut") return { ...current, cueOut: value };
      if (id === "introEnd") return { ...current, introEnd: value };
      return { ...current, outroStart: value };
    });
    setFields((current) => ({ ...current, [id]: toField(value) }));
  };

  const onField = (key: keyof MarkValues, value: string) => {
    setFields((current) => ({ ...current, [key]: value }));
    const ms = parseField(value);
    if (ms == null) {
      if ((key === "introEnd" || key === "outroStart") && value.trim() === "") {
        setDraft((current) => (current ? { ...current, [key]: null } : current));
      }
      return;
    }
    setDraft((current) => (current ? { ...current, [key]: ms } : current));
  };

  const onFieldBlur = (key: keyof MarkValues) => {
    if (!draft) return;
    setFields((current) => ({ ...current, [key]: toField(draft[key]) }));
  };

  const setNoIntro = (no: boolean) => {
    if (!draft) return;
    const value = no ? null : (analysisData?.intro_ms ?? draft.cueIn);
    setDraft({ ...draft, introEnd: value });
    setFields((current) => ({ ...current, introEnd: toField(value) }));
  };

  const setNoOutro = (no: boolean) => {
    if (!draft) return;
    const value = no ? null : (analysisData?.outro_ms ?? draft.cueOut);
    setDraft({ ...draft, outroStart: value });
    setFields((current) => ({ ...current, outroStart: toField(value) }));
  };

  const startPreview = async (fromMs: number) => {
    setError(null);
    setPositionMs(fromMs);
    try {
      const state = await preview.play(trackId, Math.max(0, Math.round(fromMs)));
      setPlaying(state.playing);
      setPositionMs(state.position_ms);
    } catch (e) {
      setPlaying(false);
      setPositionMs(null);
      setError(String(e));
    }
  };

  const stopPreview = async () => {
    try { await preview.stop(); } catch (e) { setError(String(e)); }
    setPlaying(false);
    setPositionMs(null);
  };

  const close = () => {
    if (draft && savedRef.current && !sameMarks(draft, savedRef.current)
      && !window.confirm(t("analysis.cueEditor.discard"))) return;
    void preview.stop().catch(() => {});
    onClose();
  };

  const detect = async () => {
    setAnalysing(true);
    setError(null);
    setNotice(null);
    try {
      const result = await analysis.analyzeTrack(trackId, false);
      setAnalysisData(result);
      showDraft(fromAnalysis(result));
      setMeta(metaFromAnalysis(result));
    } catch (e) {
      setError(String(e));
    } finally {
      setAnalysing(false);
    }
  };

  const save = async () => {
    if (!draft) return;
    const payload: TrackCues = {
      track_id: trackId,
      duration_ms: durationMs,
      cue_in_ms: Math.round(draft.cueIn),
      cue_out_ms: Math.round(draft.cueOut),
      intro_end_ms: draft.introEnd == null ? null : Math.round(draft.introEnd),
      outro_start_ms: draft.outroStart == null ? null : Math.round(draft.outroStart),
      source: "Manual",
      confidence: meta?.confidence ?? 0,
      loudness_lufs: meta?.loudness ?? null,
      peak_dbfs: meta?.peak ?? null,
      analyzed_at: meta?.analyzedAt ?? null,
    };
    try {
      const stored = await analysis.setTrackCues(payload);
      const marks = fromCues(stored);
      setCues(stored);
      showDraft(marks);
      setSaved(marks);
      setMeta(metaFromCues(stored));
      setNotice(t("analysis.cueEditor.saved"));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const clear = async () => {
    if (!window.confirm(t("analysis.cueEditor.confirmClear"))) return;
    try {
      await analysis.clearTrackCues(trackId);
      setCues(null);
      const marks = analysisData
        ? fromAnalysis(analysisData)
        : { cueIn: 0, cueOut: durationMs, introEnd: null, outroStart: null };
      showDraft(marks);
      setSaved(marks);
      setMeta(analysisData ? metaFromAnalysis(analysisData) : null);
      setNotice(t("analysis.cueEditor.cleared"));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const marks: WaveformMark[] = draft ? [
    { id: "cueIn", ms: draft.cueIn, color: "var(--text-secondary)", label: t("analysis.cueEditor.cueIn") },
    { id: "introEnd", ms: draft.introEnd, color: "var(--green)", label: t("analysis.cueEditor.introEnd") },
    { id: "outroStart", ms: draft.outroStart, color: "var(--yellow)", label: t("analysis.cueEditor.outroStart") },
    { id: "cueOut", ms: draft.cueOut, color: "var(--text-secondary)", label: t("analysis.cueEditor.cueOut") },
  ] : [];

  const previewReady = devices?.preview != null;
  const gain = normalization?.enabled ? gainDb(normalization, meta?.loudness ?? null, meta?.peak ?? null) : null;
  const introRoom = draft?.introEnd != null ? Math.max(0, draft.introEnd - draft.cueIn) / 1000 : null;
  const outroRoom = draft?.outroStart != null ? Math.max(0, draft.cueOut - draft.outroStart) / 1000 : null;

  return (
    <Dialog title={t("analysis.cueEditor.title", { title })} width={920} onClose={close}>
      {analysing && analysisData && (
        <div style={S.top}>
          <span style={S.spinner}>{t("analysis.cueEditor.analysing")}</span>
        </div>
      )}

      <div style={S.wave}>
        {analysisData ? (
          <Waveform
            peaks={analysisData.peaks}
            durationMs={durationMs}
            marks={marks}
            onMove={setMark}
            positionMs={positionMs}
            height={170}
          />
        ) : (
          <div style={{ ...S.hint, padding: 24, textAlign: "center" }}>
            {analysing ? t("analysis.cueEditor.analysing") : t("analysis.cueEditor.noWaveform")}
          </div>
        )}
      </div>

      {analysisData && analysisData.vocal_timeline.length > 0 && (
        <div style={S.stripWrap}>
          <div style={S.stripLabel}>{t("analysis.cueEditor.vocalTimeline")}</div>
          <div style={S.strip} aria-hidden="true">
            {analysisData.vocal_timeline.map((level, i) => (
              <span
                key={i}
                title={t("analysis.cueEditor.voiceCell", { second: i, level })}
                style={{
                  flex: 1, minWidth: 0,
                  background: `rgba(63,185,80,${0.06 + (Math.max(0, Math.min(9, level)) / 9) * 0.8})`,
                }}
              />
            ))}
          </div>
        </div>
      )}

      {draft && (
        <>
          <div style={S.fields}>
            <label style={S.field}>{t("analysis.cueEditor.cueIn")}
              <input style={S.input} value={fields.cueIn} inputMode="decimal"
                onChange={(e) => onField("cueIn", e.target.value)}
                onBlur={() => onFieldBlur("cueIn")} />
            </label>
            <label style={S.field}>{t("analysis.cueEditor.introEnd")}
              <input style={S.input} value={fields.introEnd} inputMode="decimal"
                onChange={(e) => onField("introEnd", e.target.value)}
                onBlur={() => onFieldBlur("introEnd")} />
            </label>
            <label style={S.field}>{t("analysis.cueEditor.outroStart")}
              <input style={S.input} value={fields.outroStart} inputMode="decimal"
                onChange={(e) => onField("outroStart", e.target.value)}
                onBlur={() => onFieldBlur("outroStart")} />
            </label>
            <label style={S.field}>{t("analysis.cueEditor.cueOut")}
              <input style={S.input} value={fields.cueOut} inputMode="decimal"
                onChange={(e) => onField("cueOut", e.target.value)}
                onBlur={() => onFieldBlur("cueOut")} />
            </label>
          </div>

          <div style={S.checks}>
            <label style={S.check}>
              <input type="checkbox" checked={draft.introEnd == null} onChange={(e) => setNoIntro(e.target.checked)} />
              {t("analysis.cueEditor.noIntro")}
            </label>
            <label style={S.check}>
              <input type="checkbox" checked={draft.outroStart == null} onChange={(e) => setNoOutro(e.target.checked)} />
              {t("analysis.cueEditor.noOutro")}
            </label>
          </div>
        </>
      )}

      {meta && (
        <div style={S.info}>
          <span>{t("analysis.cueEditor.source", { source: t(`analysis.source.${meta.source}`) })}</span>
          <span>{t("analysis.cueEditor.confidence", { value: Math.round(meta.confidence * 100) })}</span>
          <span>{t("analysis.cueEditor.loudness", { value: numberText(meta.loudness) })}</span>
          <span>{t("analysis.cueEditor.peak", { value: numberText(meta.peak) })}</span>
          <span>
            {gain == null
              ? t("analysis.cueEditor.gainOff")
              : t("analysis.cueEditor.gain", { value: signed(gain) })}
          </span>
          {draft && (
            <span style={S.room}>
              {t("analysis.cueEditor.room", {
                intro: introRoom == null ? "—" : introRoom.toFixed(1),
                outro: outroRoom == null ? "—" : outroRoom.toFixed(1),
              })}
            </span>
          )}
        </div>
      )}

      <div style={S.prelisten}>
        <span style={S.subtitle}>{t("analysis.cueEditor.prelisten")}</span>
        <button style={S.btn} disabled={!previewReady || !draft}
          onClick={() => draft && startPreview(draft.cueIn)}>
          {t("analysis.cueEditor.fromStart")}
        </button>
        <button style={S.btn} disabled={!previewReady || !draft || draft.introEnd == null}
          onClick={() => draft?.introEnd != null && startPreview(Math.max(draft.cueIn, draft.introEnd - 5000))}>
          {t("analysis.cueEditor.introTail")}
        </button>
        <button style={S.btn} disabled={!previewReady || !draft || draft.outroStart == null}
          onClick={() => draft?.outroStart != null && startPreview(draft.outroStart)}>
          {t("analysis.cueEditor.fromVoiceEnd")}
        </button>
        <button style={S.btn} disabled={!previewReady || !playing} onClick={stopPreview}>
          {t("analysis.cueEditor.stop")}
        </button>
        {!previewReady && <span style={S.hint}>{t("analysis.cueEditor.noPreview")}</span>}
      </div>

      {error && <div style={S.error}>{error}</div>}
      {notice && <div style={S.notice}>{notice}</div>}

      <div style={S.actions}>
        <button style={S.btn} onClick={detect} disabled={analysing}>
          {t("analysis.cueEditor.detectAgain")}
        </button>
        <button style={S.btn} onClick={clear} disabled={cues == null}>
          {t("analysis.cueEditor.clearMarks")}
        </button>
        <span style={S.spacer} />
        <button style={S.btn} onClick={close}>{t("menu.close")}</button>
        <button style={S.btnPrimary} onClick={save} disabled={!draft || analysing}>
          {t("menu.save")}
        </button>
      </div>
    </Dialog>
  );
}
