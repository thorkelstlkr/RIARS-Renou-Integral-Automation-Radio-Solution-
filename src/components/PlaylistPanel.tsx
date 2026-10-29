import { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useCommand } from "../commands";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { KIND_COLORS, type PlaylistItem } from "../automation/api";
import { startTimes, type EngineState } from "../automation/useOnAir";
import { analysis, preview, type TrackCues } from "../analysis/api";
import { live } from "../live/api";
import CueEditor from "../analysis/CueEditor";
import ContextMenu, { type ContextEntry } from "../menu/ContextMenu";
import { usePlaylistDrop } from "./usePlaylistDrop";
import { metadata, type TrackMetadata } from "./metadataReview";

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
  list: { flex: 1, overflowY: "auto", outline: "none" },
  selected: { background: "rgba(233,69,96,0.18)" },
  empty: { padding: "24px 16px", textAlign: "center", color: "var(--text-muted)", fontSize: 13 },
  row: {
    display: "flex", alignItems: "center", padding: "8px 16px", cursor: "pointer", userSelect: "none",
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
  alb: { flex: 1, paddingLeft: 8, color: "var(--text-secondary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  year: { width: 44, textAlign: "right" as const, fontFamily: "var(--font-mono)", fontSize: 12, color: "var(--text-secondary)" },
  gen: { flex: 0.6, minWidth: 0, paddingLeft: 12, color: "var(--text-secondary)", fontSize: 12, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
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
  // Insertion line of a drag: above a row, or below the last one.
  dropBefore: { boxShadow: "inset 0 2px 0 var(--accent)" },
  dropAfter: { boxShadow: "inset 0 -2px 0 var(--accent)" },
  dropEmpty: { outline: "2px dashed var(--accent)", outlineOffset: -8, color: "var(--text-primary)" },
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
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [anchor, setAnchor] = useState<string | null>(null);
  const [ctx, setCtx] = useState<{ x: number; y: number } | null>(null);
  const [cueItem, setCueItem] = useState<{ trackId: number; title: string } | null>(null);
  const listRef = useRef<HTMLDivElement>(null);

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
  const drop = usePlaylistDrop(listRef, items.map((item) => item.id));

  // Forget the rows that left the playlist.
  useEffect(() => {
    setSelected((prev) => {
      if (prev.size === 0) return prev;
      const valid = new Set(items.map((item) => item.id));
      let changed = false;
      const next = new Set<string>();
      for (const id of prev) {
        if (valid.has(id)) next.add(id);
        else changed = true;
      }
      return changed ? next : prev;
    });
    setAnchor((prev) => (prev && items.some((item) => item.id === prev) ? prev : null));
  }, [items]);

  // Positions are looked up when an action runs, from the list as it is then.
  const itemsRef = useRef(items);
  itemsRef.current = items;
  const indexOf = (id: string) => itemsRef.current.findIndex((item) => item.id === id);
  const selectedIds = () => items.filter((item) => selected.has(item.id)).map((item) => item.id);

  const selectedIndexes = () =>
    items.map((item, i) => (selected.has(item.id) ? i : -1)).filter((i) => i >= 0);

  const selectOnly = (id: string) => {
    setSelected(new Set([id]));
    setAnchor(id);
  };

  const selectRange = (from: number, to: number) => {
    const set = new Set<string>();
    for (let i = Math.min(from, to); i <= Math.max(from, to); i++) set.add(items[i].id);
    setSelected(set);
  };

  const onRowClick = (e: React.MouseEvent, i: number) => {
    const id = items[i].id;
    if (e.shiftKey && anchor) {
      const a = items.findIndex((item) => item.id === anchor);
      if (a >= 0) { selectRange(a, i); return; }
    }
    if (e.ctrlKey || e.metaKey) {
      setSelected((prev) => {
        const next = new Set(prev);
        if (next.has(id)) next.delete(id);
        else next.add(id);
        return next;
      });
      setAnchor(id);
      return;
    }
    selectOnly(id);
  };

  const onRowContext = (e: React.MouseEvent, i: number) => {
    e.preventDefault();
    const id = items[i].id;
    if (!selected.has(id)) selectOnly(id);
    setCtx({ x: e.clientX, y: e.clientY });
  };

  const moveSelection = (dir: 1 | -1, extend: boolean) => {
    if (items.length === 0) return;
    const idxs = selectedIndexes();
    const current = idxs.length > 0 ? idxs[idxs.length - 1] : -1;
    const next = Math.max(0, Math.min(items.length - 1, current + dir));
    if (!extend) {
      selectOnly(items[next].id);
      return;
    }
    const a = anchor ? items.findIndex((item) => item.id === anchor) : -1;
    const from = a >= 0 ? a : current >= 0 ? current : next;
    selectRange(from, next);
    setAnchor(items[from].id);
  };

  const openMenuAtSelection = () => {
    if (items.length === 0) return;
    let idxs = selectedIndexes();
    if (idxs.length === 0) {
      selectOnly(items[0].id);
      idxs = [0];
    }
    const row = listRef.current?.querySelector<HTMLElement>(`[data-row-index="${idxs[0]}"]`);
    const rect = row?.getBoundingClientRect();
    setCtx({
      x: rect ? rect.left + 40 : 80,
      y: rect ? rect.top + rect.height / 2 : 80,
    });
  };

  const onListKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable) return;
    const idxs = selectedIndexes();
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      const dir = e.key === "ArrowDown" ? 1 : -1;
      if (e.altKey) {
        if (idxs.length !== 1) return;
        const i = idxs[0];
        if (dir === -1 && i > 0) void run("playlist_move", { from: i, to: i - 1 });
        if (dir === 1 && i < items.length - 1) void run("playlist_move", { from: i, to: i + 1 });
      } else {
        moveSelection(dir, e.shiftKey);
      }
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      setSelected(new Set(items.map((item) => item.id)));
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      setSelected(new Set());
      setAnchor(null);
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      if (idxs.length === 1) void run("playlist_play", { index: idxs[0] });
      return;
    }
    if (e.key === "Delete") {
      e.preventDefault();
      if (idxs.length > 0) void run("playlist_remove_many", { ids: selectedIds() });
      return;
    }
    if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
      e.preventDefault();
      openMenuAtSelection();
    }
  };

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

  // Album, year and genre come from the library, also by path, so they follow the
  // library: reloaded when the playlist or the library changes (metadata completed).
  const [meta, setMeta] = useState<Record<string, TrackMetadata>>({});
  const [libraryRev, setLibraryRev] = useState(0);
  useEffect(() => {
    const bump = () => setLibraryRev((n) => n + 1);
    window.addEventListener("library-changed", bump);
    return () => window.removeEventListener("library-changed", bump);
  }, []);
  useEffect(() => {
    if (paths.length === 0) {
      setMeta({});
      return;
    }
    let alive = true;
    metadata.forPaths(paths)
      .then((list) => {
        if (!alive) return;
        const map: Record<string, TrackMetadata> = {};
        for (const track of list) map[track.file_path] = track;
        setMeta(map);
      })
      .catch(() => { /* library unavailable */ });
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cueKey, libraryRev]);

  const clock = (ms: number | null | undefined) =>
    ms == null ? "" : new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false });

  const menuIndexes = selectedIndexes();
  const menuIndex = menuIndexes.length === 1 ? menuIndexes[0] : null;
  const menuItem = menuIndex === null ? null : items[menuIndex];
  const menuIds = selectedIds();
  // Runs `f` with the current position of the one selected item, if it is still there.
  const atItem = (f: (i: number) => void) => () => {
    const i = menuItem ? indexOf(menuItem.id) : -1;
    if (i >= 0) f(i);
  };
  const menuPaths = menuIndexes.map((i) => items[i].track_path);
  const menuTitle = menuItem ? menuItem.title : t("contextMenu.selected", { count: selected.size });

  const prelisten = async () => {
    if (!menuItem) return;
    try {
      await preview.playPath(menuItem.track_path, 0);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const toggleStopAfter = async () => {
    if (!menuItem) return;
    try {
      await live.setStopAfter(menuItem.id, !menuItem.stop_after);
      await refresh();
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const openMarks = async () => {
    if (!menuItem) return;
    try {
      // A file not in the library yet (dropped from the Folders tab, a folder not
      // scanned yet) joins it here: marks are kept per library track.
      const track = await invoke<{ id: number; title: string }>(
        "library_track_for_file", { path: menuItem.track_path });
      setCueItem({ trackId: track.id, title: track.title });
      window.dispatchEvent(new Event("library-changed"));
    } catch (e) {
      setError(String(e));
    }
  };

  const copyPaths = async () => {
    try {
      await navigator.clipboard.writeText(menuPaths.join("\n"));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const playlistMenu = (): ContextEntry[] => [
    {
      label: t("contextMenu.playNow"), shortcut: t("contextMenu.keyEnter"), disabled: menuIndex === null,
      action: atItem((i) => { void run("playlist_play", { index: i }); }),
    },
    {
      label: t("contextMenu.playNext"),
      disabled: menuIndex === null || menuIndex === current_index,
      action: () => { if (menuItem) void run("playlist_play_next", { id: menuItem.id }); },
    },
    {
      label: t("contextMenu.prelisten"), disabled: menuIndex === null,
      action: () => { void prelisten(); },
    },
    {
      label: t("contextMenu.stopAfter"), checked: menuItem?.stop_after ?? false,
      disabled: menuIndex === null,
      action: () => { void toggleStopAfter(); },
    },
    {
      label: t("contextMenu.marks"), disabled: menuIndex === null,
      action: () => { void openMarks(); },
    },
    "separator",
    {
      label: t("contextMenu.moveUp"), shortcut: "Alt+↑",
      disabled: menuIndex === null || menuIndex === 0,
      action: atItem((i) => { if (i > 0) void run("playlist_move", { from: i, to: i - 1 }); }),
    },
    {
      label: t("contextMenu.moveDown"), shortcut: "Alt+↓",
      disabled: menuIndex === null || menuIndex === items.length - 1,
      action: atItem((i) => { if (i < itemsRef.current.length - 1) void run("playlist_move", { from: i, to: i + 1 }); }),
    },
    {
      label: t("contextMenu.moveTop"), disabled: menuIndex === null || menuIndex === 0,
      action: atItem((i) => { void run("playlist_move", { from: i, to: 0 }); }),
    },
    {
      label: t("contextMenu.moveBottom"), disabled: menuIndex === null || menuIndex === items.length - 1,
      action: atItem((i) => { void run("playlist_move", { from: i, to: itemsRef.current.length - 1 }); }),
    },
    "separator",
    {
      label: t("contextMenu.removeFromPlaylist"), shortcut: t("contextMenu.keyDelete"), danger: true,
      disabled: menuIndexes.length === 0,
      action: () => { void run("playlist_remove_many", { ids: menuIds }); },
    },
    {
      label: t("contextMenu.removePlayed"), disabled: current_index === 0,
      action: () => { void run("playlist_remove_played"); },
    },
    {
      label: t("contextMenu.clearPlaylist"), danger: true,
      action: () => {
        if (window.confirm(t("contextMenu.confirmClear"))) void run("playlist_clear");
      },
    },
    "separator",
    {
      label: t("contextMenu.openFolder"), disabled: menuItem === null,
      action: () => {
        if (menuItem) invoke("reveal_in_folder", { path: menuItem.track_path }).catch((e) => setError(String(e)));
      },
    },
    {
      label: t("contextMenu.copyPath"), disabled: menuIndexes.length === 0,
      action: () => { void copyPaths(); },
    },
  ];

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
        <span style={S.alb}>{t("playlist.headerAlbum")}</span>
        <span style={S.year}>{t("playlist.headerYear")}</span>
        <span style={S.gen}>{t("playlist.headerGenre")}</span>
        <span style={S.room}>{t("playlist.headerIntro")}</span>
        <span style={S.room}>{t("playlist.headerOutro")}</span>
        <span style={S.lufs}>{t("playlist.headerLufs")}</span>
        <span style={S.dur}>{t("playlist.headerDuration")}</span>
        <span style={S.act} />
      </div>
      {error && <div style={{ ...S.foot, color: "var(--red)", borderTop: "none" }}>{error}</div>}
      <div ref={listRef} style={S.list} tabIndex={0} onKeyDown={onListKeyDown}
        onClick={() => listRef.current?.focus()}>
        {items.length === 0 && (
          <div style={{ ...S.empty, ...(drop.dropAt !== null ? S.dropEmpty : {}) }}>
            {drop.dropAt !== null ? t("playlist.drop.here") : t("playlist.empty")}
          </div>
        )}
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
            <div key={item.id} title={item.track_path} data-row-index={i}
              style={{
                ...S.row,
                ...(i === current_index ? S.active : {}),
                ...(selected.has(item.id) ? S.selected : {}),
                ...(drop.dropAt === i ? S.dropBefore : {}),
                ...(drop.dropAt === items.length && i === items.length - 1 ? S.dropAfter : {}),
              }}
              onClick={(e) => onRowClick(e, i)}
              onContextMenu={(e) => onRowContext(e, i)}
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
                {item.stop_after && (
                  <span style={S.badge} title={t("contextMenu.stopAfter")}>⏹</span>
                )}
              </span>
              <span style={S.art}>{item.artist || t("library.unknownArtist")}</span>
              <span style={S.alb} title={meta[item.track_path]?.album}>{meta[item.track_path]?.album || "—"}</span>
              <span style={S.year}>{meta[item.track_path]?.year ?? "—"}</span>
              <span style={S.gen} title={meta[item.track_path]?.genre}>{meta[item.track_path]?.genre || "—"}</span>
              <span style={S.room}>{introRoom == null ? "—" : introRoom.toFixed(1)}</span>
              <span style={S.room}>{outroRoom == null ? "—" : outroRoom.toFixed(1)}</span>
              <span style={S.lufs}>
                {cue?.loudness_lufs == null ? "—" : cue.loudness_lufs.toFixed(1)}
              </span>
              <span style={S.dur}>{fmt(item.duration_ms)}</span>
              <span style={S.act}>
                <button style={S.icon} title={t("playlist.moveUp")} disabled={i === 0}
                  onClick={(e) => { e.stopPropagation(); void run("playlist_move", { from: i, to: i - 1 }); }}>&#9650;</button>
                <button style={S.icon} title={t("playlist.moveDown")} disabled={i === items.length - 1}
                  onClick={(e) => { e.stopPropagation(); void run("playlist_move", { from: i, to: i + 1 }); }}>&#9660;</button>
                <button style={S.icon} title={t("playlist.remove")}
                  onClick={(e) => { e.stopPropagation(); void run("playlist_remove", { index: i }); }}>&#10005;</button>
              </span>
            </div>
          );
        })}
      </div>
      <div style={{ ...S.foot, display: "flex", gap: 12 }}>
        <span>{t("playlist.itemCount", { count: items.length })} · {fmt(total)}</span>
        {drop.notice && (
          <span data-testid="playlist-drop-notice"
            style={{ marginLeft: "auto", color: drop.notice.error ? "var(--red)" : "var(--text-secondary)" }}>
            {drop.notice.text}
          </span>
        )}
      </div>
      {ctx && (
        <ContextMenu x={ctx.x} y={ctx.y} title={menuTitle} items={playlistMenu()} onClose={() => setCtx(null)} />
      )}
      {cueItem && (
        <CueEditor trackId={cueItem.trackId} title={cueItem.title} onClose={() => setCueItem(null)} />
      )}
    </div>
  );
}
