import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import Dialog from "./Dialog";

// Types mirror crates/riars-core/src/rds/mod.rs and crates/riars-rds/src/lib.rs.
type Transport =
  | { type: "Tcp"; host: string; port: number }
  | { type: "Udp"; host: string; port: number }
  | { type: "Serial"; port: string; baud: number }
  | { type: "File"; path: string };
type Charset = "Rds" | "Ascii" | "Utf8";
type Protocol =
  | { type: "Uecp"; site: number; encoder: number }
  | { type: "Ascii"; template: string; charset: Charset };

interface RdsOutput { name: string; enabled: boolean; transport: Transport; protocol: Protocol }

interface RdsConfig {
  enabled: boolean;
  pi: string;
  ps: string;
  pty: number;
  tp: boolean;
  music: boolean;
  rt_template: string;
  station_text: string;
  live_text: string;
  outputs: RdsOutput[];
}

type OutputState = "Off" | "Waiting" | "Ok" | "Error";

interface RdsInfo {
  config: RdsConfig;
  status: {
    enabled: boolean;
    texts: { ps: string; rt: string };
    manual: string | null;
    manual_until: number | null;
    outputs: { name: string; state: OutputState; last_sent: number | null; last_error: string | null }[];
  };
}

const ASCII_TEMPLATE = "PS={ps}\\r\\nRT={rt}\\r\\n";

const S: Record<string, React.CSSProperties> = {
  section: { marginBottom: 16 },
  title: {
    fontSize: 12, fontWeight: 700, letterSpacing: 0.6, textTransform: "uppercase" as const,
    color: "var(--text-secondary)", marginBottom: 8,
  },
  row: { display: "flex", gap: 8, marginBottom: 8, alignItems: "flex-end" },
  field: { display: "flex", flexDirection: "column", gap: 2, flex: 1, fontSize: 11, color: "var(--text-secondary)" },
  check: { display: "flex", alignItems: "center", gap: 6, fontSize: 13 },
  input: { width: "100%", fontSize: 13 },
  hint: { fontSize: 12, color: "var(--text-muted)", marginTop: 2 },
  onAir: {
    fontFamily: "monospace", fontSize: 13, padding: "6px 8px", background: "var(--bg-input)",
    borderRadius: "var(--radius)", marginBottom: 6, whiteSpace: "pre" as const, overflow: "hidden",
  },
  output: { border: "1px solid var(--border)", borderRadius: "var(--radius)", padding: 10, marginBottom: 8 },
  dot: { width: 8, height: 8, borderRadius: "50%", display: "inline-block", marginRight: 6 },
  error: { color: "var(--red)", fontSize: 12, marginTop: 4, wordBreak: "break-word" as const },
  actions: { display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 8 },
  btn: { padding: "6px 14px", borderRadius: "var(--radius)", background: "var(--bg-panel)", fontSize: 13 },
};

const DOT: Record<OutputState, string> = {
  Off: "var(--text-muted)", Waiting: "var(--yellow)", Ok: "var(--green)", Error: "var(--red)",
};

function defaultTransport(type: Transport["type"]): Transport {
  switch (type) {
    case "Tcp": return { type, host: "", port: 7000 };
    case "Udp": return { type, host: "", port: 7000 };
    case "Serial": return { type, port: "", baud: 9600 };
    case "File": return { type, path: "" };
  }
}

/** Broadcast ▸ RDS: the station's RDS data and the encoders it goes to. */
export default function RdsDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [info, setInfo] = useState<RdsInfo | null>(null);
  const [config, setConfig] = useState<RdsConfig | null>(null);
  const [dirty, setDirtyState] = useState(false);
  const [manual, setManual] = useState("");
  const [minutes, setMinutes] = useState(10);
  const [error, setError] = useState<string | null>(null);
  // Read by the timer: settings changed elsewhere (an agent, the API) replace the form
  // only while nothing is being edited here.
  const dirtyRef = useRef(false);
  const setDirty = (value: boolean) => { dirtyRef.current = value; setDirtyState(value); };

  const refresh = useCallback(async () => {
    try {
      const next = await invoke<RdsInfo>("rds_get");
      setInfo(next);
      if (!dirtyRef.current) setConfig(next.config);
    } catch (e) { setError(String(e)); }
  }, []);

  // The status (texts on air, each output's state) changes on its own.
  useEffect(() => {
    void refresh();
    const timer = setInterval(() => { void refresh(); }, 2000);
    return () => clearInterval(timer);
  }, [refresh]);

  const edit = (change: Partial<RdsConfig>) => {
    setConfig((c) => (c ? { ...c, ...change } : c));
    setDirty(true);
  };
  const editOutput = (index: number, change: Partial<RdsOutput>) => {
    if (!config) return;
    edit({ outputs: config.outputs.map((o, i) => (i === index ? { ...o, ...change } : o)) });
  };

  const save = async () => {
    if (!config) return;
    try {
      const next = await invoke<RdsInfo>("rds_set_config", { config });
      setInfo(next);
      setConfig(next.config);
      setDirty(false);
      setError(null);
    } catch (e) { setError(String(e)); }
  };

  const sendText = async (text: string | null) => {
    try {
      setInfo(await invoke<RdsInfo>("rds_set_text", { text, minutes: text ? minutes : null }));
      if (!text) setManual("");
      setError(null);
    } catch (e) { setError(String(e)); }
  };

  const addOutput = () => {
    if (!config) return;
    const output: RdsOutput = {
      name: t("rds.outputName", { n: config.outputs.length + 1 }),
      enabled: true,
      transport: defaultTransport("Tcp"),
      protocol: { type: "Uecp", site: 0, encoder: 0 },
    };
    edit({ outputs: [...config.outputs, output] });
  };

  const text = (labelKey: string, value: string, onChange: (v: string) => void, max?: number) => (
    <label style={S.field}>
      {t(labelKey)}
      <input style={S.input} value={value} maxLength={max} onChange={(e) => onChange(e.target.value)} />
    </label>
  );
  const number = (labelKey: string, value: number, onChange: (v: number) => void, min: number, max: number, width = 90) => (
    <label style={{ ...S.field, flex: `0 0 ${width}px` }}>
      {t(labelKey)}
      <input style={S.input} type="number" min={min} max={max} value={value}
        onChange={(e) => onChange(Number(e.target.value))} />
    </label>
  );

  const until = info?.status.manual_until ? new Date(info.status.manual_until * 1000).toLocaleTimeString() : null;

  return (
    <Dialog title={t("rds.title")} width={640} onClose={onClose}>
      {!config && !error && <div style={S.hint}>{t("audio.loading")}</div>}
      {config && info && (
        <>
          <div style={S.section}>
            <div style={S.title}>{t("rds.onAir")}</div>
            <div style={S.onAir} aria-label="PS">PS  {info.status.texts.ps}</div>
            <div style={S.onAir} aria-label="RT">RT  {info.status.texts.rt}</div>
            {!info.status.enabled && <div style={S.hint}>{t("rds.offHint")}</div>}
            <div style={S.row}>
              {text("rds.manual", manual, setManual, 64)}
              {number("rds.minutes", minutes, setMinutes, 1, 1440, 80)}
              <button style={S.btn} disabled={!manual.trim()} onClick={() => sendText(manual)}>{t("rds.show")}</button>
              <button style={S.btn} disabled={!info.status.manual} onClick={() => sendText(null)}>{t("rds.clear")}</button>
            </div>
            {info.status.manual && (
              <div style={S.hint}>{until ? t("rds.manualUntil", { time: until }) : t("rds.manualOn")}</div>
            )}
          </div>

          <div style={S.section}>
            <div style={S.title}>{t("rds.station")}</div>
            <label style={{ ...S.check, marginBottom: 8 }}>
              <input type="checkbox" checked={config.enabled} onChange={(e) => edit({ enabled: e.target.checked })} />
              {t("rds.enabled")}
            </label>
            <div style={S.row}>
              <label style={{ ...S.field, flex: "0 0 90px" }}>
                PI
                <input style={S.input} value={config.pi} maxLength={4}
                  onChange={(e) => edit({ pi: e.target.value.toUpperCase() })} />
              </label>
              <label style={{ ...S.field, flex: "0 0 140px" }}>
                PS
                <input style={S.input} value={config.ps} maxLength={8} onChange={(e) => edit({ ps: e.target.value })} />
              </label>
              {number("rds.pty", config.pty, (pty) => edit({ pty }), 0, 31, 70)}
              <label style={S.check}>
                <input type="checkbox" checked={config.tp} onChange={(e) => edit({ tp: e.target.checked })} />TP
              </label>
              <label style={S.check}>
                <input type="checkbox" checked={config.music} onChange={(e) => edit({ music: e.target.checked })} />
                {t("rds.music")}
              </label>
            </div>
            <div style={S.hint}>{t("rds.piHint")}</div>
          </div>

          <div style={S.section}>
            <div style={S.title}>{t("rds.texts")}</div>
            <div style={S.row}>{text("rds.template", config.rt_template, (rt_template) => edit({ rt_template }))}</div>
            <div style={S.row}>{text("rds.stationText", config.station_text, (station_text) => edit({ station_text }), 64)}</div>
            <div style={S.row}>{text("rds.liveText", config.live_text, (live_text) => edit({ live_text }), 64)}</div>
            <div style={S.hint}>{t("rds.textsHint")}</div>
          </div>

          <div style={S.section}>
            <div style={S.title}>{t("rds.outputs")}</div>
            {config.outputs.length === 0 && <div style={{ ...S.hint, marginBottom: 8 }}>{t("rds.noOutputs")}</div>}
            {config.outputs.map((output, i) => {
              const status = dirty ? undefined : info.status.outputs[i];
              const tr = output.transport;
              const pr = output.protocol;
              return (
                <div key={i} style={S.output}>
                  <div style={S.row}>
                    {text("rds.name", output.name, (name) => editOutput(i, { name }), 64)}
                    <label style={S.check}>
                      <input type="checkbox" checked={output.enabled} onChange={(e) => editOutput(i, { enabled: e.target.checked })} />
                      {t("rds.outputOn")}
                    </label>
                    <button style={S.btn} onClick={() => edit({ outputs: config.outputs.filter((_, j) => j !== i) })}>
                      {t("rds.remove")}
                    </button>
                  </div>
                  <div style={S.row}>
                    <label style={{ ...S.field, flex: "0 0 160px" }}>
                      {t("rds.connection")}
                      <select style={S.input} value={tr.type}
                        onChange={(e) => {
                          const transport = defaultTransport(e.target.value as Transport["type"]);
                          const protocol: Protocol = transport.type === "File"
                            ? { type: "Ascii", template: "{rt}", charset: "Utf8" } : pr;
                          editOutput(i, { transport, protocol });
                        }}>
                        <option value="Tcp">TCP</option>
                        <option value="Udp">UDP</option>
                        <option value="Serial">{t("rds.serial")}</option>
                        <option value="File">{t("rds.file")}</option>
                      </select>
                    </label>
                    {(tr.type === "Tcp" || tr.type === "Udp") && (
                      <>
                        {text("rds.host", tr.host, (host) => editOutput(i, { transport: { ...tr, host } }))}
                        {number("rds.port", tr.port, (port) => editOutput(i, { transport: { ...tr, port } }), 1, 65535)}
                      </>
                    )}
                    {tr.type === "Serial" && (
                      <>
                        {text("rds.serialPort", tr.port, (port) => editOutput(i, { transport: { ...tr, port } }))}
                        <label style={{ ...S.field, flex: "0 0 100px" }}>
                          {t("rds.baud")}
                          <select style={S.input} value={tr.baud}
                            onChange={(e) => editOutput(i, { transport: { ...tr, baud: Number(e.target.value) } })}>
                            {[1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200].map((b) => <option key={b} value={b}>{b}</option>)}
                          </select>
                        </label>
                      </>
                    )}
                    {tr.type === "File" && text("rds.path", tr.path, (path) => editOutput(i, { transport: { ...tr, path } }))}
                  </div>
                  <div style={S.row}>
                    <label style={{ ...S.field, flex: "0 0 160px" }}>
                      {t("rds.protocol")}
                      <select style={S.input} value={pr.type} disabled={tr.type === "File"}
                        onChange={(e) => editOutput(i, {
                          protocol: e.target.value === "Uecp"
                            ? { type: "Uecp", site: 0, encoder: 0 }
                            : { type: "Ascii", template: ASCII_TEMPLATE, charset: "Rds" },
                        })}>
                        <option value="Uecp">UECP</option>
                        <option value="Ascii">{t("rds.ascii")}</option>
                      </select>
                    </label>
                    {pr.type === "Uecp" && (
                      <>
                        {number("rds.site", pr.site, (site) => editOutput(i, { protocol: { ...pr, site } }), 0, 1023)}
                        {number("rds.encoder", pr.encoder, (encoder) => editOutput(i, { protocol: { ...pr, encoder } }), 0, 63)}
                      </>
                    )}
                    {pr.type === "Ascii" && (
                      <>
                        {text("rds.commands", pr.template, (template) => editOutput(i, { protocol: { ...pr, template } }))}
                        <label style={{ ...S.field, flex: "0 0 110px" }}>
                          {t("rds.charset")}
                          <select style={S.input} value={pr.charset}
                            onChange={(e) => editOutput(i, { protocol: { ...pr, charset: e.target.value as Charset } })}>
                            <option value="Rds">RDS</option>
                            <option value="Ascii">ASCII</option>
                            <option value="Utf8">UTF-8</option>
                          </select>
                        </label>
                      </>
                    )}
                  </div>
                  {status && (
                    <div style={S.hint}>
                      <span style={{ ...S.dot, background: DOT[status.state] }} />
                      {t(`rds.state.${status.state}`)}
                      {status.last_sent ? ` · ${t("rds.lastSent", { time: new Date(status.last_sent * 1000).toLocaleTimeString() })}` : ""}
                      {status.last_error && <div style={S.error}>{status.last_error}</div>}
                    </div>
                  )}
                </div>
              );
            })}
            <button style={S.btn} onClick={addOutput} disabled={config.outputs.length >= 8}>{t("rds.addOutput")}</button>
          </div>
        </>
      )}

      {error && <div style={S.error}>{error}</div>}

      <div style={S.actions}>
        <button style={S.btn} onClick={save} disabled={!dirty}>{t("rds.save")}</button>
        <button style={S.btn} onClick={onClose}>{t("menu.close")}</button>
      </div>
    </Dialog>
  );
}
