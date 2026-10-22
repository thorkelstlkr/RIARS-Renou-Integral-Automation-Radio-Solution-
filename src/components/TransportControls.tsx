import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  volume: number;
  onVolumeChange: (v: number) => void;
}

const S: Record<string, React.CSSProperties> = {
  wrap: { display: "flex", alignItems: "center", gap: 4 },
  btn: {
    width: 40, height: 40, display: "flex", alignItems: "center", justifyContent: "center",
    borderRadius: "var(--radius)", fontSize: 18,
  },
  vol: { display: "flex", alignItems: "center", gap: 6, marginLeft: 12 },
  pct: { fontSize: 12, fontFamily: "var(--font-mono)", color: "var(--text-secondary)", minWidth: 36 },
};

export default function TransportControls({ volume, onVolumeChange }: Props) {
  const { t } = useTranslation();

  const play = useCallback(() => invoke("engine_play"), []);
  const pause = useCallback(() => invoke("engine_pause"), []);
  const stop = useCallback(() => invoke("engine_stop"), []);
  const next = useCallback(() => invoke("engine_next"), []);

  const handleVolume = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const v = parseFloat(e.target.value);
      onVolumeChange(v);
      invoke("engine_set_volume", { volume: v });
    },
    [onVolumeChange],
  );

  return (
    <div style={S.wrap}>
      <button style={S.btn} onClick={play} title={t("transport.play")}>&#9654;</button>
      <button style={S.btn} onClick={pause} title={t("transport.pause")}>&#10074;&#10074;</button>
      <button style={S.btn} onClick={stop} title={t("transport.stop")}>&#9724;</button>
      <button style={S.btn} onClick={next} title={t("transport.next")}>&#9197;</button>
      <div style={S.vol}>
        <span style={{ fontSize: 14 }}>{t("transport.volume")}</span>
        <input type="range" min={0} max={1} step={0.01} value={volume} onChange={handleVolume}
          style={{ width: 100, accentColor: "var(--accent)" }} />
        <span style={S.pct}>{Math.round(volume * 100)}%</span>
      </div>
    </div>
  );
}
