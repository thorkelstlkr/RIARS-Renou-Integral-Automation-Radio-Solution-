import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

// Serde's externally tagged enums, as the Rust side sends them.
type Trigger =
  | { Time: string }
  | { Daily: { time: string } }
  | { Weekly: { day: string; time: string } }
  | { Interval: { minutes: number } };
type Action =
  | { PlayTrack: { track_path: string } }
  | { SetVolume: { level: number } }
  | { FadeToVolume: { level: number; duration_ms: number } }
  | { PlayPlaylist: { playlist_id: string } }
  | "StartStreaming"
  | "StopStreaming";

interface ScheduledEvent {
  id: string;
  name: string;
  trigger: Trigger;
  action: Action;
  enabled: boolean;
}

type TriggerKind = "Time" | "Daily" | "Weekly" | "Interval";
type ActionKind = "PlayTrack" | "SetVolume" | "FadeToVolume" | "StartStreaming" | "StopStreaming";
const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

const S: Record<string, React.CSSProperties> = {
  panel: { display: "flex", flexDirection: "column", height: "100%" },
  list: { flex: 1, overflowY: "auto" },
  empty: { padding: "24px 16px", textAlign: "center", color: "var(--text-muted)", fontSize: 13 },
  row: {
    display: "flex", alignItems: "center", gap: 10, padding: "8px 16px",
    borderBottom: "1px solid rgba(255,255,255,0.03)",
  },
  name: { flex: 1, fontSize: 13, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  when: { width: 170, fontSize: 12, fontFamily: "var(--font-mono)", color: "var(--text-secondary)" },
  what: { flex: 1, fontSize: 12, color: "var(--text-secondary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  icon: { padding: "0 6px", fontSize: 12, background: "transparent", color: "var(--text-muted)" },
  form: {
    display: "flex", flexWrap: "wrap", alignItems: "flex-end", gap: 8, padding: "10px 16px",
    borderTop: "1px solid var(--border)", background: "var(--bg-secondary)",
  },
  field: { display: "flex", flexDirection: "column", gap: 2, fontSize: 11, color: "var(--text-secondary)" },
  input: { fontSize: 13 },
  btn: { padding: "5px 12px", borderRadius: "var(--radius)", fontSize: 13, background: "var(--bg-panel)" },
  error: { padding: "6px 16px", fontSize: 12, color: "var(--red)" },
};

export default function SchedulePanel() {
  const { t } = useTranslation();
  const [events, setEvents] = useState<ScheduledEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [triggerKind, setTriggerKind] = useState<TriggerKind>("Daily");
  const [time, setTime] = useState("08:00:00");
  const [day, setDay] = useState("Mon");
  const [minutes, setMinutes] = useState(60);
  const [actionKind, setActionKind] = useState<ActionKind>("PlayTrack");
  const [path, setPath] = useState("");
  const [level, setLevel] = useState(80);
  const [seconds, setSeconds] = useState(5);

  const run = useCallback(async (cmd: string, args?: Record<string, unknown>) => {
    try {
      setEvents(await invoke<ScheduledEvent[]>(cmd, args));
      setError(null);
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    }
  }, []);

  // Poll: one-shot events disable themselves when they fire.
  useEffect(() => {
    run("schedule_get");
    const id = setInterval(() => run("schedule_get"), 2000);
    return () => clearInterval(id);
  }, [run]);

  const describeTrigger = (tr: Trigger) => {
    if ("Time" in tr) return t("schedule.onceAt", { time: tr.Time });
    if ("Daily" in tr) return t("schedule.dailyAt", { time: tr.Daily.time });
    if ("Weekly" in tr) return t("schedule.weeklyAt", { day: t(`schedule.days.${tr.Weekly.day}`), time: tr.Weekly.time });
    return t("schedule.everyMinutes", { count: tr.Interval.minutes });
  };
  const describeAction = (a: Action) => {
    if (a === "StartStreaming") return t("schedule.startStreaming");
    if (a === "StopStreaming") return t("schedule.stopStreaming");
    if ("PlayTrack" in a) return `${t("schedule.playTrack")}: ${a.PlayTrack.track_path.split(/[\\/]/).pop()}`;
    if ("SetVolume" in a) return `${t("schedule.setVolume")}: ${Math.round(a.SetVolume.level * 100)}%`;
    if ("FadeToVolume" in a) return `${t("schedule.fadeToVolume")}: ${Math.round(a.FadeToVolume.level * 100)}% / ${a.FadeToVolume.duration_ms / 1000} s`;
    return t("schedule.playPlaylist");
  };

  const add = async () => {
    // <input type="time"> drops the seconds when they are :00.
    const hms = time.length === 5 ? `${time}:00` : time;
    const trigger: Trigger =
      triggerKind === "Time" ? { Time: hms }
      : triggerKind === "Daily" ? { Daily: { time: hms } }
      : triggerKind === "Weekly" ? { Weekly: { day, time: hms } }
      : { Interval: { minutes } };
    const action: Action =
      actionKind === "PlayTrack" ? { PlayTrack: { track_path: path } }
      : actionKind === "SetVolume" ? { SetVolume: { level: level / 100 } }
      : actionKind === "FadeToVolume" ? { FadeToVolume: { level: level / 100, duration_ms: seconds * 1000 } }
      : actionKind;
    if (actionKind === "PlayTrack" && !path) {
      setError(t("schedule.pickTrack"));
      return;
    }
    if (await run("schedule_add", { name, trigger, action })) setName("");
  };

  const pickTrack = async () => {
    const file = await open({ multiple: false, filters: [{ name: "Audio", extensions: ["mp3", "flac", "wav", "ogg", "aac", "m4a"] }] });
    if (typeof file === "string") setPath(file);
  };

  return (
    <div style={S.panel}>
      <div style={S.list}>
        {events.length === 0 && <div style={S.empty}>{t("schedule.empty")}</div>}
        {events.map((ev) => (
          <div key={ev.id} style={{ ...S.row, opacity: ev.enabled ? 1 : 0.5 }}>
            <input type="checkbox" checked={ev.enabled} title={t("schedule.enabled")}
              onChange={() => run("schedule_toggle", { id: ev.id })} />
            <span style={S.name}>{ev.name}</span>
            <span style={S.when}>{describeTrigger(ev.trigger)}</span>
            <span style={S.what} title={JSON.stringify(ev.action)}>{describeAction(ev.action)}</span>
            <button style={S.icon} title={t("schedule.remove")} onClick={() => run("schedule_remove", { id: ev.id })}>&#10005;</button>
          </div>
        ))}
      </div>
      {error && <div style={S.error}>{error}</div>}
      <div style={S.form}>
        <label style={S.field}>{t("schedule.name")}
          <input style={{ ...S.input, width: 140 }} value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <label style={S.field}>{t("schedule.when")}
          <select style={S.input} value={triggerKind} onChange={(e) => setTriggerKind(e.target.value as TriggerKind)}>
            <option value="Time">{t("schedule.once")}</option>
            <option value="Daily">{t("schedule.daily")}</option>
            <option value="Weekly">{t("schedule.weekly")}</option>
            <option value="Interval">{t("schedule.interval")}</option>
          </select>
        </label>
        {triggerKind === "Weekly" && (
          <label style={S.field}>{t("schedule.day")}
            <select style={S.input} value={day} onChange={(e) => setDay(e.target.value)}>
              {DAYS.map((d) => <option key={d} value={d}>{t(`schedule.days.${d}`)}</option>)}
            </select>
          </label>
        )}
        {triggerKind === "Interval" ? (
          <label style={S.field}>{t("schedule.minutes")}
            <input style={{ ...S.input, width: 70 }} type="number" min={1} value={minutes} onChange={(e) => setMinutes(Number(e.target.value))} />
          </label>
        ) : (
          <label style={S.field}>{t("schedule.time")}
            <input style={S.input} type="time" step={1} value={time} onChange={(e) => setTime(e.target.value)} />
          </label>
        )}
        <label style={S.field}>{t("schedule.action")}
          <select style={S.input} value={actionKind} onChange={(e) => setActionKind(e.target.value as ActionKind)}>
            <option value="PlayTrack">{t("schedule.playTrack")}</option>
            <option value="SetVolume">{t("schedule.setVolume")}</option>
            <option value="FadeToVolume">{t("schedule.fadeToVolume")}</option>
            <option value="StartStreaming">{t("schedule.startStreaming")}</option>
            <option value="StopStreaming">{t("schedule.stopStreaming")}</option>
          </select>
        </label>
        {actionKind === "PlayTrack" && (
          <button style={S.btn} onClick={pickTrack} title={path}>
            {path ? path.split(/[\\/]/).pop() : t("schedule.chooseFile")}
          </button>
        )}
        {(actionKind === "SetVolume" || actionKind === "FadeToVolume") && (
          <label style={S.field}>{t("schedule.level")}
            <input style={{ ...S.input, width: 60 }} type="number" min={0} max={100} value={level} onChange={(e) => setLevel(Number(e.target.value))} />
          </label>
        )}
        {actionKind === "FadeToVolume" && (
          <label style={S.field}>{t("schedule.seconds")}
            <input style={{ ...S.input, width: 60 }} type="number" min={1} value={seconds} onChange={(e) => setSeconds(Number(e.target.value))} />
          </label>
        )}
        <button style={S.btn} onClick={add}>{t("schedule.add")}</button>
      </div>
    </div>
  );
}
