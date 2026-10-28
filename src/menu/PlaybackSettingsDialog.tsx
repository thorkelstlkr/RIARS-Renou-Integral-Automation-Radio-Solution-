import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import Dialog from "./Dialog";
import { audio, type NormalizationConfig } from "../analysis/api";

type Curve = "Linear" | "EqualPower" | "SCurve";
interface Crossfade { enabled: boolean; duration_ms: number; curve: Curve }
interface DeadAir { enabled: boolean; threshold_db: number; seconds: number; action: "Skip" | "Alert" }
type Focus = "crossfade" | "deadAir" | "normalization";

const S: Record<string, React.CSSProperties> = {
  section: { marginBottom: 18 },
  title: { fontSize: 12, fontWeight: 700, letterSpacing: 0.6, textTransform: "uppercase", color: "var(--text-secondary)", marginBottom: 8 },
  row: { display: "flex", alignItems: "center", gap: 10, marginBottom: 8, fontSize: 13 },
  label: { width: 150, color: "var(--text-secondary)" },
  hint: { fontSize: 12, color: "var(--text-muted)", marginTop: 4 },
  actions: { display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 8 },
  btn: { padding: "6px 14px", borderRadius: "var(--radius)", background: "var(--bg-panel)", fontSize: 13 },
  primary: { padding: "6px 14px", borderRadius: "var(--radius)", background: "var(--accent)", color: "#fff", fontSize: 13 },
  error: { color: "var(--red)", fontSize: 12, marginTop: 6 },
};

/** Playback ▸ Crossfade… / Silence detector… / Normalisation… (one dialog, three sections). */
export default function PlaybackSettingsDialog({ focus, onClose }: { focus: Focus; onClose: () => void }) {
  const { t } = useTranslation();
  const [fade, setFade] = useState<Crossfade | null>(null);
  const [deadAir, setDeadAir] = useState<DeadAir | null>(null);
  const [normalization, setNormalization] = useState<NormalizationConfig | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<Crossfade>("crossfade_get").then(setFade).catch((e) => setError(String(e)));
    invoke<DeadAir>("dead_air_get").then(setDeadAir).catch((e) => setError(String(e)));
    audio.getNormalization().then(setNormalization).catch((e) => setError(String(e)));
  }, []);

  const save = async () => {
    try {
      if (fade) await invoke("crossfade_set", { config: fade });
      if (deadAir) await invoke("dead_air_set", { config: deadAir });
      if (normalization) await audio.setNormalization(normalization);
      onClose();
    } catch (e) { setError(String(e)); }
  };

  const title = focus === "crossfade"
    ? t("menu.crossfadeTitle")
    : focus === "deadAir"
      ? t("menu.deadAirTitle")
      : t("menu.normalizationTitle");

  return (
    <Dialog title={title} onClose={onClose}>
      {fade && (
        <div style={S.section}>
          <div style={S.title}>{t("menu.crossfadeTitle")}</div>
          <label style={S.row}>
            <input type="checkbox" checked={fade.enabled} onChange={(e) => setFade({ ...fade, enabled: e.target.checked })} />
            {t("menu.crossfadeOn")}
          </label>
          <label style={S.row}>
            <span style={S.label}>{t("menu.crossfadeSeconds")}</span>
            <input type="number" min={0} max={30} step={0.5} style={{ width: 90 }} value={fade.duration_ms / 1000}
              onChange={(e) => setFade({ ...fade, duration_ms: Math.round(Math.min(30, Math.max(0, Number(e.target.value))) * 1000) })} />
          </label>
          <label style={S.row}>
            <span style={S.label}>{t("menu.crossfadeCurve")}</span>
            <select value={fade.curve} onChange={(e) => setFade({ ...fade, curve: e.target.value as Curve })}>
              <option value="EqualPower">{t("menu.curveEqualPower")}</option>
              <option value="Linear">{t("menu.curveLinear")}</option>
              <option value="SCurve">{t("menu.curveSCurve")}</option>
            </select>
          </label>
          <div style={S.hint}>{t("menu.crossfadeHint")}</div>
        </div>
      )}
      {deadAir && (
        <div style={S.section}>
          <div style={S.title}>{t("menu.deadAirTitle")}</div>
          <label style={S.row}>
            <input type="checkbox" checked={deadAir.enabled} onChange={(e) => setDeadAir({ ...deadAir, enabled: e.target.checked })} />
            {t("menu.deadAirOn")}
          </label>
          <label style={S.row}>
            <span style={S.label}>{t("menu.deadAirSeconds")}</span>
            <input type="number" min={1} max={600} style={{ width: 90 }} value={deadAir.seconds}
              onChange={(e) => setDeadAir({ ...deadAir, seconds: Math.min(600, Math.max(1, Math.round(Number(e.target.value)))) })} />
          </label>
          <label style={S.row}>
            <span style={S.label}>{t("menu.deadAirThreshold")}</span>
            <input type="number" min={-90} max={0} style={{ width: 90 }} value={deadAir.threshold_db}
              onChange={(e) => setDeadAir({ ...deadAir, threshold_db: Math.min(0, Math.max(-90, Number(e.target.value))) })} />
          </label>
          <label style={S.row}>
            <span style={S.label}>{t("menu.deadAirAction")}</span>
            <select value={deadAir.action} onChange={(e) => setDeadAir({ ...deadAir, action: e.target.value as DeadAir["action"] })}>
              <option value="Skip">{t("menu.deadAirSkip")}</option>
              <option value="Alert">{t("menu.deadAirAlert")}</option>
            </select>
          </label>
        </div>
      )}
      {normalization && (
        <div style={S.section}>
          <div style={S.title}>{t("menu.normalizationTitle")}</div>
          <label style={S.row}>
            <input type="checkbox" checked={normalization.enabled}
              onChange={(e) => setNormalization({ ...normalization, enabled: e.target.checked })} />
            {t("menu.normalizationOn")}
          </label>
          <label style={S.row}>
            <span style={S.label}>{t("menu.normalizationTarget")}</span>
            <input type="number" min={-30} max={-5} step={0.5} style={{ width: 90 }} value={normalization.target_lufs}
              onChange={(e) => setNormalization({
                ...normalization,
                target_lufs: Math.min(-5, Math.max(-30, Number(e.target.value))),
              })} />
          </label>
          <label style={S.row}>
            <span style={S.label}>{t("menu.normalizationMaxGain")}</span>
            <input type="number" min={0} max={20} step={0.5} style={{ width: 90 }} value={normalization.max_gain_db}
              onChange={(e) => setNormalization({
                ...normalization,
                max_gain_db: Math.min(20, Math.max(0, Number(e.target.value))),
              })} />
          </label>
          <label style={S.row}>
            <span style={S.label}>{t("menu.normalizationMaxCut")}</span>
            <input type="number" min={0} max={30} step={0.5} style={{ width: 90 }} value={normalization.max_cut_db}
              onChange={(e) => setNormalization({
                ...normalization,
                max_cut_db: Math.min(30, Math.max(0, Number(e.target.value))),
              })} />
          </label>
          <label style={S.row}>
            <span style={S.label}>{t("menu.normalizationPeak")}</span>
            <input type="number" min={-6} max={0} step={0.5} style={{ width: 90 }} value={normalization.peak_ceiling_dbfs}
              onChange={(e) => setNormalization({
                ...normalization,
                peak_ceiling_dbfs: Math.min(0, Math.max(-6, Number(e.target.value))),
              })} />
          </label>
          <div style={S.hint}>{t("menu.normalizationHint")}</div>
        </div>
      )}
      {error && <div style={S.error}>{error}</div>}
      <div style={S.actions}>
        <button style={S.btn} onClick={onClose}>{t("menu.cancel")}</button>
        <button style={S.primary} onClick={save} disabled={!fade || !deadAir || !normalization}>{t("menu.save")}</button>
      </div>
    </Dialog>
  );
}
