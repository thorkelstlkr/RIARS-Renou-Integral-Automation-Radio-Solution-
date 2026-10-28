import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import Dialog from "./Dialog";
import { audio, type AudioDevices } from "../analysis/api";

const S: Record<string, React.CSSProperties> = {
  section: { marginBottom: 18 },
  title: {
    fontSize: 12, fontWeight: 700, letterSpacing: 0.6, textTransform: "uppercase" as const,
    color: "var(--text-secondary)", marginBottom: 8,
  },
  select: { width: "100%", fontSize: 13 },
  hint: { fontSize: 12, color: "var(--text-muted)", marginTop: 4 },
  warning: { fontSize: 12, color: "var(--yellow)", marginTop: 10, marginBottom: 4 },
  error: { color: "var(--red)", fontSize: 12, marginTop: 6, wordBreak: "break-word" as const },
  actions: { display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 8 },
  btn: { padding: "6px 14px", borderRadius: "var(--radius)", background: "var(--bg-panel)", fontSize: 13 },
};

/** Playback ▸ Sound cards: the on-air card (next start) and the pre-listen card. */
export default function AudioSettingsDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [devices, setDevices] = useState<AudioDevices | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    audio.devices().then((value) => { if (alive) setDevices(value); }).catch((e) => { if (alive) setError(String(e)); });
    return () => { alive = false; };
  }, []);

  const setMain = async (value: string) => {
    try {
      setDevices(await audio.setMainDevice(value || null));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const setPreview = async (value: string) => {
    try {
      setDevices(await audio.setPreviewDevice(value || null));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const onAir = devices ? devices.main ?? devices.default : null;
  const same = devices?.preview != null && devices.preview === onAir;

  return (
    <Dialog title={t("audio.title")} width={520} onClose={onClose}>
      {!devices && !error && <div style={S.hint}>{t("audio.loading")}</div>}
      {devices && (
        <>
          <div style={S.section}>
            <div style={S.title}>{t("audio.onAir")}</div>
            <select style={S.select} value={devices.main ?? ""} aria-label={t("audio.onAir")}
              onChange={(e) => setMain(e.target.value)}>
              <option value="">{t("audio.systemDefault")}</option>
              {devices.devices.map((device) => <option key={device} value={device}>{device}</option>)}
            </select>
            {devices.default && <div style={S.hint}>{t("audio.defaultIs", { device: devices.default })}</div>}
            {devices.main_in_use && <div style={S.hint}>{t("audio.inUse", { device: devices.main_in_use })}</div>}
            <div style={S.hint}>{t("audio.restartNote")}</div>
          </div>

          <div style={S.section}>
            <div style={S.title}>{t("audio.preview")}</div>
            <select style={S.select} value={devices.preview ?? ""} aria-label={t("audio.preview")}
              onChange={(e) => setPreview(e.target.value)}>
              <option value="">{t("audio.off")}</option>
              {devices.devices.map((device) => <option key={device} value={device}>{device}</option>)}
            </select>
            <div style={S.hint}>{t("audio.previewNote")}</div>
          </div>

          {same && <div style={S.warning} role="status">{t("audio.sameWarning")}</div>}
        </>
      )}

      {error && <div style={S.error}>{error}</div>}

      <div style={S.actions}>
        <button style={S.btn} onClick={onClose}>{t("menu.close")}</button>
      </div>
    </Dialog>
  );
}
