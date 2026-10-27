import { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useCommand } from "../commands";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { KIND_COLORS, type PlaylistItem } from "../automation/api";
import { startTimes, type EngineState } from "../automation/useOnAir";
import { analysis, type TrackCues } from "../analysis/api";

interface Playlist {
  name?: string;
  items: PlaylistItem[];
  current_index: number;
}

interface SavedPlaylist {
  id: string;
  name: string;
  items: number;
  duration_ms: number;
}

const S: Record<string, React.CSSProperties> = {
  panel: { display: "flex", flexDirection: "column", height: "100%" },
  toolbar: {
    display: "flex", alignItems: "center", gap: 8, padding: "10px 16px",
    borderBottom: "1px solid var(--border)",
  },
  title: { fontSize: 14, fontWeight: 600, flex: 1 },
  btn: {
    padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12,
    background: "var(--bg-panel)",
  },
  hdr: {
    display: "flex", padding: "6px 16px", fontSize: 11, fontWeight: 600,
    color: "var(--text-muted)", textTransform: "uppercase", letterSpacing: 0.5,
    borderBottom: "1px solid var(--border)",
  },
  list: { flex: 1, overflowY: "auto" },
  empty: { padding: "24px 16px", textAlign: "center", color: "var(--text-muted)", fontSize: 13 },
  row: {
    display: "flex", alignItems: "center", padding: "8px 16px", cursor: "pointer",
    borderBottom: "1px solid rgba(255,255,255,0.03)",
  },
  active: {
    background: "var(--bg-panel)", borderLeft: "3px solid var(--accent)",
  },
  num: { width: 36, color: "var(--text-muted)", fontFamily: "var(--font-mono)", fontSize: 12 },
  start: { width: 64, color: "var(--text-secondary)", fontFamily: "var(--font-mono)", fontSize: 12 },
  kind: { width: 8, height: 8, borderRadius: "50%", display: "inline-block", marginRight: 8, flexShrink: 0 },
  tit: { flex: 2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  art: { flex: 1, color: "var(--text-secondary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  room: {
    width: 48, textAlign: "right" as const, fontFamily: "var(--font-mono)", fontSize: 11,
    color: "var(--text-secondary)",
  },
  lufs: {
    width: 46, textAlign: "right" as const, fontFamily: "var(--font-mono)", fontSize: 11,
    color: "var(--text-muted)",
  },
  badge: {
    marginLeft: 6, padding: "0 4px", borderRadius: 3, border: "1px solid var(--yellow)",
    color: "var(--yellow)", fontSize: 10, lineHeight: "15px", whiteSpace: "nowrap" as const, flexShrink: 0,
  },
  dur: { width: 70, textAlign: "right" as const, fontFamily: "var(--font-mono)", fontSize: 12, color: "var(--text-secondary)" },
  act: { width: 72, display: "flex", justifyContent: "flex-end", gap: 2 },
  icon: { padding: "0 6px", fontSize: 12, background: "transparent", color: "var(--text-muted)" },
  menu: {
    display: "flex", alignItems: "center", gap: 8, padding: "8px 16px",
    borderBottom: "1px solid var(--border)", background: "var(--bg-secondary)",
  },
  foot: { padding: "6px 16px", fontSize: 12, color: "var(--text-muted)", borderTop: "1px solid var(--border)" },
};

const fmt = (ms: number) => {
  if (ms <= 0) return "--:--";
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
};

const overlaySide = (startMs: number, cue: TrackCues | undefined, durationMs: number): "intro" | "outro" => {
  if (cue?.intro_end_ms != null && startMs <= cue.intro_end_ms) return "intro";
  if (cue?.outro_start_ms != null && startMs >= cue.outro_start_ms) return "outro";
  return startMs < durationMs / 2 ? "intro" : "outro";
};

export default function PlaylistPanel() {
  const { t } = useTranslation();
  const [playlist, setPlaylist] = useState<Playlist>({ items: [], current_index: 0 });
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<SavedPlaylist[]>([]);
  const [menu, setMenu] = useState<"none" | "save" | "open">("none");
  const [saveName, setSaveName] = useState("");
  const [engine, setEngine] = useState<EngineState | null>(null);
  const [cues, setCues] = useState<Record<string, TrackCues>>({});

  // Start times follow the position of the track on air.
  useEffect(() => {
    invoke<EngineState>("engine_get_state").then(setEngine).catch(() => {});
    const off = listen<EngineState>("engine-state", (e) => setEngine(e.payload));
    return () => { off.then((f) => f()); };
  }, []);

  const openMenu = useCallback(async (which: "save" | "open") => {
    if (menu === which) { setMenu("none"); return; }
    try { setSaved(await invoke<SavedPlaylist[]>("playlists_list")); } catch (e) { setError(String(e)); }
    setMenu(which);
  }, [menu]);

  const savePlaylist = useCallback(async () => {
    try {
      setSaved(await invoke<SavedPlaylist[]>("playlists_save", { name: saveName }));
      setMenu("none");
      setSaveName("");
      setError(null);
    } catch (e) { setError(String(e)); }
  }, [saveName]);

  const loadPlaylist = useCallback(async (id: string) => {
    try {
      setPlaylist(await invoke<Playlist>("playlists_load", { id, play: false }));
      setMenu("none");
      setError(null);
    } catch (e) { setError(String(e)); }
  }, []);

  const deletePlaylist = useCallback(async (id: string) => {
    try { setSaved(await invoke<SavedPlaylist[]>("playlists_delete", { id })); } catch (e) { setError(String(e)); }
  }, []);

  // Revision of the playlist on screen: the poll only transfers it when it changed.
  const revision = useRef<number | null>(null);
  const refresh = useCallback(async () => {
    try {
      const change = await invoke<{ revision: number; playlist: Playlist } | null>(
        "playlist_changes", { since: revision.current });
      if (change) {
        revision.current = change.revision;
        setPlaylist(change.playlist);
      }
    } catch { /* not ready */ }
  }, []);

  // The backend announces every playlist change (auto-advance, additions...).
  useEffect(() => {
    refresh();
    const unlisten = listen("playlist-revision", () => refresh());
    window.addEventListener("playlist-changed", refresh);
    return () => { unlisten.then((f) => f()); window.removeEventListener("playlist-changed", refresh); };
  }, [refresh]);

  const run = useCallback(async (cmd: string, args?: Record<string, unknown>) => {
    try {
      const result = await invoke<Playlist | null>(cmd, args);
      if (result) setPlaylist(result); else await refresh();
      setError(null);
    } catch (e) {
      setError(cmd === "playlist_play" ? t("library.loadError", { error: String(e) }) : String(e));
    }
  }, [refresh, t]);

  // File menu and shortcuts.
  useCommand("playlist.open", useCallback(() => { void openMenu("open"); }, [openMenu]));
  useCommand("playlist.save", useCallback(() => { void openMenu("save"); }, [openMenu]));
  useCommand("playlist.clear", useCallback(() => { void run("playlist_clear"); }, [run]));

  const { items, current_index } = playlist;
  const total = items.reduce((sum, i) => sum + i.duration_ms, 0);
  const starts = engine ? startTimes(items, current_index, engine) : [];

  // Cue points for the Intro / Outro / LUFS columns, by file path so items added by
  // hand or from a saved playlist (no track id) get them too. Reloaded when the
  // playlist changes.
  const paths = Array.from(new Set(items.map((item) => item.track_path))).slice(0, 1000);
  const cueKey = paths.join("|");
  useEffect(() => {
    if (paths.length === 0) {
      setCues({});
      return;
    }
    let alive = true;
    analysis.pathsCues(paths)
      .then((list) => {
        if (!alive) return;
        const map: Record<string, TrackCues> = {};
        for (const found of list) map[found.path] = found.cues;
        setCues(map);
      })
      .catch(() => { /* analysis unavailable */ });
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cueKey]);

  const clock = (ms: number | null | undefined) =>
    ms == null ? "" : new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false });

  return (
    <div style={S.panel}>
      <div style={S.toolbar}>
        <h2 style={S.title}>{t("playlist.title")}</h2>
        <button style={S.btn} onClick={() => openMenu("open")}>{t("playlist.open")}</button>
        <button style={S.btn} onClick={() => openMenu("save")} disabled={items.length === 0}>{t("playlist.save")}</button>
        <button style={S.btn} onClick={() => run("playlist_clear")} disabled={items.length === 0}>
          {t("playlist.clear")}
        </button>
      </div>
      {menu === "save" && (
        <div style={S.menu}>
          <input style={{ flex: 1, fontSize: 13 }} placeholder={t("playlist.saveName")} value={saveName} autoFocus
            onChange={(e) => setSaveName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && savePlaylist()} />
          <button style={S.btn} onClick={savePlaylist}>{t("playlist.save")}</button>
        </div>
      )}
      {menu === "open" && (
        <div style={{ ...S.menu, flexDirection: "column", alignItems: "stretch" }}>
          {saved.length === 0 && <div style={{ fontSize: 12, color: "var(--text-muted)" }}>{t("playlist.noSaved")}</div>}
          {saved.map((p) => (
            <div key={p.id} style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <button style={{ ...S.btn, flex: 1, textAlign: "left" }} onClick={() => loadPlaylist(p.id)}>{p.name}</button>
              <span style={{ fontSize: 12, color: "var(--text-muted)", fontFamily: "var(--font-mono)" }}>
                {t("playlist.itemCount", { count: p.items })} · {fmt(p.duration_ms)}
              </span>
              <button style={S.icon} title={t("playlist.deleteSaved")} onClick={() => deletePlaylist(p.id)}>&#10005;</button>
            </div>
          ))}
        </div>
      )}
      <div style={S.hdr}>
        <span style={S.num}>{t("playlist.headerNumber")}</span>
        <span style={S.start}>{t("playlist.headerStart")}</span>
        <span style={S.tit}>{t("playlist.headerTitle")}</span>
        <span style={S.art}>{t("playlist.headerArtist")}</span>
        <span style={S.room}>{t("playlist.headerIntro")}</span>
        <span style={S.room}>{t("playlist.headerOutro")}</span>
        <span style={S.lufs}>{t("playlist.headerLufs")}</span>
        <span style={S.dur}>{t("playlist.headerDuration")}</span>
        <span style={S.act} />
      </div>
      {error && <div style={{ ...S.foot, color: "var(--red)", borderTop: "none" }}>{error}</div>}
      <div style={S.list}>
        {items.length === 0 && <div style={S.empty}>{t("playlist.empty")}</div>}
        {items.map((item, i) => {
          const cue = cues[item.track_path];
          const introRoom = cue?.intro_end_ms != null
            ? Math.max(0, cue.intro_end_ms - cue.cue_in_ms) / 1000
            : null;
          const outroRoom = cue?.outro_start_ms != null
            ? Math.max(0, cue.cue_out_ms - cue.outro_start_ms) / 1000
            : null;
          const side = item.overlay ? overlaySide(item.overlay.start_ms, cue, item.duration_ms) : null;
          return (
            <div key={item.id} title={item.track_path}
              style={{ ...S.row, ...(i === current_index ? S.active : {}) }}
              onDoubleClick={() => run("playlist_play", { index: i })}>
              <span style={S.num}>{i + 1}</span>
              <span style={S.start}>{clock(starts[i])}</span>
              <span style={{ ...S.tit, display: "flex", alignItems: "center" }}>
                <span style={{ ...S.kind, background: KIND_COLORS[item.kind ?? "Music"] }}
                  title={t(`automation.kinds.${item.kind ?? "Music"}`) + (item.generated ? ` · ${t("playlist.generated")}` : "")} />
                <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{item.title}</span>
                {item.overlay && side && (
                  <span style={S.badge} title={item.overlay.item.title}>
                    🎙︎ {side === "intro" ? t("playlist.overlayIntro") : t("playlist.overlayOutro")}
                  </span>
                )}
              </span>
              <span style={S.art}>{item.artist || t("library.unknownArtist")}</span>
              <span style={S.room}>{introRoom == null ? "—" : introRoom.toFixed(1)}</span>
              <span style={S.room}>{outroRoom == null ? "—" : outroRoom.toFixed(1)}</span>
              <span style={S.lufs}>
                {cue?.loudness_lufs == null ? "—" : cue.loudness_lufs.toFixed(1)}
              </span>
              <span style={S.dur}>{fmt(item.duration_ms)}</span>
              <span style={S.act}>
                <button style={S.icon} title={t("playlist.moveUp")} disabled={i === 0}
                  onClick={() => run("playlist_move", { from: i, to: i - 1 })}>&#9650;</button>
                <button style={S.icon} title={t("playlist.moveDown")} disabled={i === items.length - 1}
                  onClick={() => run("playlist_move", { from: i, to: i + 1 })}>&#9660;</button>
                <button style={S.icon} title={t("playlist.remove")}
                  onClick={() => run("playlist_remove", { index: i })}>&#10005;</button>
              </span>
            </div>
          );
        })}
      </div>
      <div style={S.foot}>
        {t("playlist.itemCount", { count: items.length })} · {fmt(total)}
      </div>
    </div>
  );
}
