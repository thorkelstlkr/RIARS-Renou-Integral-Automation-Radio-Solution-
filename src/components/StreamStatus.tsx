import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useCommand } from "../commands";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type StreamState = "Disconnected" | "Connecting" | "Connected" | "Error";

interface IcecastConfig {
  host: string;
  port: number;
  mount: string;
  username: string;
  password: string;
  bitrate: number;
  format: "Mp3";
  station_name: string;
  station_description: string;
  station_genre: string;
  station_url: string;
}

interface StreamInfo {
  available: boolean;
  status: {
    state: StreamState;
    url: string | null;
    bytes_sent: number;
    last_error: string | null;
    title: string | null;
  } | null;
}

const DEFAULTS: IcecastConfig = {
  host: "localhost", port: 8000, mount: "/stream", username: "source", password: "",
  bitrate: 128, format: "Mp3", station_name: "RIARS", station_description: "",
  station_genre: "", station_url: "",
};
// Everything but the password is remembered between sessions.
const STORAGE_KEY = "riars.stream.config";

const S: Record<string, React.CSSProperties> = {
  anchor: { position: "relative", marginLeft: "auto" },
  badge: {
    display: "flex", alignItems: "center", gap: 6, padding: "6px 12px",
    borderRadius: "var(--radius)", background: "var(--bg-input)", cursor: "pointer",
  },
  dot: { width: 8, height: 8, borderRadius: "50%" },
  label: { fontSize: 11, fontWeight: 700, letterSpacing: 0.5 },
  panel: {
    position: "absolute", right: 0, top: "calc(100% + 6px)", zIndex: 10, width: 320,
    padding: 12, background: "var(--bg-secondary)", border: "1px solid var(--border)",
    borderRadius: "var(--radius)", display: "flex", flexDirection: "column", gap: 8,
    boxShadow: "0 8px 24px rgba(0,0,0,0.4)",
  },
  row: { display: "flex", gap: 8 },
  field: { display: "flex", flexDirection: "column", gap: 2, flex: 1, fontSize: 11, color: "var(--text-secondary)" },
  input: { width: "100%", fontSize: 13 },
  info: { fontSize: 12, color: "var(--text-secondary)", wordBreak: "break-all" },
  btn: { padding: "6px 10px", borderRadius: "var(--radius)", fontSize: 13, background: "var(--bg-panel)" },
};

function loadSaved(): IcecastConfig {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    return { ...DEFAULTS, ...saved, password: "" };
  } catch {
    return DEFAULTS;
  }
}

const fmtBytes = (n: number) =>
  n > 1_048_576 ? `${(n / 1_048_576).toFixed(1)} MB` : `${Math.round(n / 1024)} KB`;

export default function StreamStatus() {
  const { t } = useTranslation();
  const [info, setInfo] = useState<StreamInfo | null>(null);
  const [open, setOpen] = useState(false);
  const [config, setConfig] = useState<IcecastConfig>(loadSaved);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try { setInfo(await invoke<StreamInfo>("stream_get")); } catch { /* not ready */ }
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = listen<StreamInfo>("stream-status", (e) => setInfo(e.payload));
    return () => { unlisten.then((f) => f()); };
  }, [refresh]);

  const set = <K extends keyof IcecastConfig>(key: K, value: IcecastConfig[K]) =>
    setConfig((c) => ({ ...c, [key]: value }));

  const connect = async () => {
    try {
      const { password: _omit, ...rest } = config;
      try { localStorage.setItem(STORAGE_KEY, JSON.stringify(rest)); } catch { /* storage off */ }
      setInfo(await invoke<StreamInfo>("stream_start", { config }));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const disconnect = async () => {
    try { setInfo(await invoke<StreamInfo>("stream_stop")); } catch (e) { setError(String(e)); }
  };

  // Broadcast menu: settings open the panel; start uses the saved settings when the
  // password is there, otherwise it asks for it in the panel.
  useCommand("stream.settings", useCallback(() => setOpen(true), []));
  useCommand("stream.start", () => { if (config.password && config.host) void connect(); else setOpen(true); });
  useCommand("stream.stop", () => void disconnect());

  const state = info?.status?.state ?? "Disconnected";
  const active = state !== "Disconnected";
  const color = { Connected: "var(--green)", Connecting: "var(--yellow)", Error: "var(--red)", Disconnected: "var(--text-muted)" }[state];
  const label = !info?.available ? t("stream.unavailable")
    : { Connected: t("stream.streaming"), Connecting: t("stream.connecting"), Error: t("stream.error"), Disconnected: t("stream.offline") }[state];

  const text = (key: keyof IcecastConfig, labelKey: string, type = "text") => (
    <label style={S.field}>
      {t(labelKey)}
      <input style={S.input} type={type} value={String(config[key])} disabled={active}
        onChange={(e) => set(key, (type === "number" ? Number(e.target.value) : e.target.value) as never)} />
    </label>
  );

  return (
    <div style={S.anchor}>
      <button style={S.badge} onClick={() => setOpen((o) => !o)} title={t("stream.settings")}>
        <span style={{ ...S.dot, background: color }} />
        <span style={S.label}>{label}</span>
      </button>
      {open && (
        <div style={S.panel}>
          {!info?.available && <div style={S.info}>{t("stream.noDevice")}</div>}
          <div style={S.row}>{text("host", "stream.host")}<div style={{ width: 80 }}>{text("port", "stream.port", "number")}</div></div>
          <div style={S.row}>{text("mount", "stream.mount")}
            <label style={{ ...S.field, flex: "0 0 80px" }}>
              {t("stream.bitrate")}
              <select style={S.input} value={config.bitrate} disabled={active}
                onChange={(e) => set("bitrate", Number(e.target.value))}>
                {[64, 96, 128, 160, 192, 256, 320].map((b) => <option key={b} value={b}>{b} kbps</option>)}
              </select>
            </label>
          </div>
          <div style={S.row}>{text("username", "stream.username")}{text("password", "stream.password", "password")}</div>
          {text("station_name", "stream.stationName")}
          {text("station_genre", "stream.genre")}
          {info?.status?.url && active && <div style={S.info}>{info.status.url}</div>}
          {state === "Connected" && info?.status && (
            <div style={S.info}>
              {t("stream.sent", { size: fmtBytes(info.status.bytes_sent) })}
              {info.status.title ? ` · ${info.status.title}` : ""}
            </div>
          )}
          {(error || info?.status?.last_error) && (
            <div style={{ ...S.info, color: "var(--red)" }}>{error ?? info?.status?.last_error}</div>
          )}
          {active
            ? <button style={S.btn} onClick={disconnect}>{t("stream.disconnect")}</button>
            : <button style={S.btn} onClick={connect} disabled={!info?.available}>{t("stream.connect")}</button>}
        </div>
      )}
    </div>
  );
}
