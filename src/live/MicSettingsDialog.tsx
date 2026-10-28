import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import Dialog from "../menu/Dialog";
import { live, type MicConfig } from "./api";

const S: Record<string, React.CSSProperties> = {
  section: { marginBottom: 16 },
  title: {
    fontSize: 12, fontWeight: 700, letterSpacing: 0.6, textTransform: "uppercase" as const,
    color: "var(--text-secondary)", marginBottom: 8,
  },
  field: { display: "flex", flexDirection: "column" as const, gap: 6, marginBottom: 12 },
  label: { fontSize: 12, color: "var(--text-secondary)" },
  row: { display: "flex", alignItems: "center", gap: 8 },
  range: { flex: 1 },
  value: { width: 62, textAlign: "right" as const, fontFamily: "var(--font-mono)", fontSize: 12 },
  check: { display: "flex", alignItems: "center", gap: 8, fontSize: 13, marginBottom: 10 },
  select: { width: "100%", fontSize: 13 },
  hint: { fontSize: 12, color: "var(--text-muted)", marginTop: 4 },
  warning: { fontSize: 12, color: "var(--yellow)", marginTop: 6 },
  error: { color: "var(--red)", fontSize: 12, marginTop: 6, wordBreak: "break-word" as const },
  actions: { display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 8 },
  btn: { padding: "6px 14px", borderRadius: "var(--radius)", background: "var(--bg-panel)", fontSize: 13 },
  primary: { padding: "6px 14px", borderRadius: "var(--radius)", background: "var(--accent)", color: "#fff", fontSize: 13 },
};

const dB = (value: number) => `${value > 0 ? "+" : ""}${value.toFixed(1)} dB`;

/** Microphone settings: input card, processing, music ducking and remote access. */
export default function MicSettingsDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [config, setConfig] = useState<MicConfig | null>(null);
  const [inputs, setInputs] = useState<{ devices: string[]; default: string | null }>({ devices: [], default: null });
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let alive = true;
    Promise.all([live.micConfig(), live.inputDevices()])
      .then(([mic, cards]) => {
        if (!alive) return;
        setConfig(mic);
        setInputs(cards);
      })
      .catch((e) => { if (alive) setError(String(e)); });
    return () => { alive = false; };
  }, []);

  const patch = (values: Partial<MicConfig>) => setConfig((prev) => (prev ? { ...prev, ...values } : prev));

  const save = async () => {
    if (!config) return;
    setSaving(true);
    try {
      await live.setMicConfig(config);
      onClose();
    } catch (e) {
      setError(String(e));
      setSaving(false);
    }
  };

  const devices = Array.from(
    new Set([...inputs.devices, ...(config?.device ? [config.device] : [])]),
  );

  return (
    <Dialog title={t("live.micSettings")} width={520} onClose={onClose}>
      {!config && !error && <div style={S.hint}>{t("audio.loading")}</div>}
      {config && (
        <>
          <div style={S.section}>
            <div style={S.field}>
              <span style={S.label}>{t("live.inputDevice")}</span>
              <select style={S.select} value={config.device ?? ""} aria-label={t("live.inputDevice")}
                onChange={(e) => patch({ device: e.target.value || null })}>
                <option value="">{t("live.systemDefault")}</option>
                {devices.map((device) => <option key={device} value={device}>{device}</option>)}
              </select>
              {inputs.default && <div style={S.hint}>{t("audio.defaultIs", { device: inputs.default })}</div>}
            </div>

            <div style={S.field}>
              <span style={S.label}>{t("live.gain")}</span>
              <div style={S.row}>
                <input style={S.range} type="range" min={-20} max={30} step={1}
                  aria-label={t("live.gain")} value={config.gain_db}
                  onChange={(e) => patch({ gain_db: Number(e.target.value) })} />
                <span style={S.value}>{dB(config.gain_db)}</span>
              </div>
            </div>

            <label style={S.check}>
              <input type="checkbox" checked={config.high_pass}
                onChange={(e) => patch({ high_pass: e.target.checked })} />
              {t("live.highPass")}
            </label>

            <label style={S.check}>
              <input type="checkbox" checked={config.gate}
                onChange={(e) => patch({ gate: e.target.checked })} />
              {t("live.gate")}
            </label>

            <div style={S.field}>
              <span style={S.label}>{t("live.gateThreshold")}</span>
              <div style={S.row}>
                <input style={S.range} type="range" min={-80} max={-10} step={1}
                  aria-label={t("live.gateThreshold")} value={config.gate_threshold_db}
                  disabled={!config.gate}
                  onChange={(e) => patch({ gate_threshold_db: Number(e.target.value) })} />
                <span style={S.value}>{config.gate_threshold_db.toFixed(0)} dBFS</span>
              </div>
            </div>

            <label style={S.check}>
              <input type="checkbox" checked={config.compressor}
                onChange={(e) => patch({ compressor: e.target.checked })} />
              {t("live.compressor")}
            </label>
          </div>

          <div style={S.section}>
            <div style={S.title}>{t("live.ducking")}</div>
            <div style={S.field}>
              <span style={S.label}>{t("live.duckMusic")}</span>
              <div style={S.row}>
                <input style={S.range} type="range" min={0} max={40} step={1}
                  aria-label={t("live.duckMusic")} value={config.duck_db}
                  onChange={(e) => patch({ duck_db: Number(e.target.value) })} />
                <span style={S.value}>{config.duck_db.toFixed(0)} dB</span>
              </div>
            </div>
            <div style={S.field}>
              <span style={S.label}>{t("live.duckMode")}</span>
              <select style={S.select} value={config.duck} aria-label={t("live.duckMode")}
                onChange={(e) => patch({ duck: e.target.value as MicConfig["duck"] })}>
                <option value="WhileOpen">{t("live.duckWhileOpen")}</option>
                <option value="WhileSpeaking">{t("live.duckWhileSpeaking")}</option>
              </select>
            </div>
          </div>

          <label style={S.check}>
            <input type="checkbox" checked={config.allow_remote}
              onChange={(e) => patch({ allow_remote: e.target.checked })} />
            {t("live.allowRemote")}
          </label>
          <div style={S.warning}>{t("live.allowRemoteHint")}</div>
        </>
      )}

      {error && <div style={S.error}>{error}</div>}

      <div style={S.actions}>
        <button style={S.btn} onClick={onClose}>{t("menu.cancel")}</button>
        <button style={S.primary} onClick={save} disabled={!config || saving}>{t("menu.save")}</button>
      </div>
    </Dialog>
  );
}
