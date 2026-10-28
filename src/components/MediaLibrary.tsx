import { useState, useCallback, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useCommand } from "../commands";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import CueEditor from "../analysis/CueEditor";
import { analysis, preview } from "../analysis/api";
import ContextMenu, { type ContextEntry } from "../menu/ContextMenu";
import FolderExplorer from "./FolderExplorer";
import { useLibraryScan } from "./useLibraryScan";

interface Track {
  id: number;
  title: string;
  artist: string;
  duration_ms: number;
  file_path: string;
}

const S: Record<string, React.CSSProperties> = {
  wrap: { display: "flex", flexDirection: "column", height: "100%" },
  toolbar: {
    display: "flex", alignItems: "center", justifyContent: "space-between",
    padding: "10px 12px", borderBottom: "1px solid var(--border)",
  },
  title: { fontSize: 14, fontWeight: 600 },
  btn: { padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12, background: "var(--bg-panel)" },
  tabs: { display: "flex", gap: 2, padding: "0 12px", borderBottom: "1px solid var(--border)" },
  tab: {
    padding: "8px 14px", fontSize: 12, color: "var(--text-muted)", background: "transparent",
    border: "none", borderBottom: "2px solid transparent", marginBottom: -1,
  },
  tabActive: { color: "var(--text-primary)", borderBottom: "2px solid var(--accent)" },
  folderPane: { flex: 1, minHeight: 0, display: "flex", flexDirection: "column" },
  search: { padding: "8px 12px", borderBottom: "1px solid var(--border)" },
  input: { width: "100%", fontSize: 13 },
  notice: { padding: "6px 12px", fontSize: 12, borderBottom: "1px solid var(--border)", wordBreak: "break-word" },
  list: { flex: 1, overflowY: "auto", outline: "none" },
  selected: { background: "rgba(233,69,96,0.18)" },
  count: { padding: "4px 12px", fontSize: 11, color: "var(--text-muted)", borderBottom: "1px solid var(--border)" },
  empty: { padding: "24px 12px", textAlign: "center", color: "var(--text-muted)", fontSize: 13 },
  track: {
    display: "flex", alignItems: "center", gap: 6, padding: "8px 12px", cursor: "pointer", userSelect: "none",
    borderBottom: "1px solid rgba(255,255,255,0.03)",
  },
  trackBody: { flex: 1, minWidth: 0 },
  name: { fontSize: 13, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  meta: { display: "flex", justifyContent: "space-between", fontSize: 11, color: "var(--text-muted)", marginTop: 2 },
  marks: {
    padding: "2px 7px", borderRadius: 4, fontSize: 13, color: "var(--text-muted)",
    flexShrink: 0, background: "transparent",
  },
};

/// Tracks per page: the list grows as it is scrolled, never the whole library at once.
const PAGE = 100;

type Tab = "library" | "folders";

const readTab = (): Tab => {
  try {
    return localStorage.getItem("riars.library.tab") === "folders" ? "folders" : "library";
  } catch {
    return "library";
  }
};

export default function MediaLibrary() {
  const { t } = useTranslation();
  const [tracks, setTracks] = useState<Track[]>([]);
  const [total, setTotal] = useState(0);
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(false);
  const [notice, setNotice] = useState<{ text: string; error?: boolean } | null>(null);
  const [cueTrack, setCueTrack] = useState<Track | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [anchor, setAnchor] = useState<number | null>(null);
  const [ctx, setCtx] = useState<{ x: number; y: number } | null>(null);
  const [tab, setTab] = useState<Tab>(readTab);
  const listRef = useRef<HTMLDivElement>(null);
  // Which search the loaded pages belong to, so a late page of an old search is dropped.
  const query = useRef({ search: "", busy: false, generation: 0 });

  // Load one page; `reset` starts over (new search, rescanned library).
  const loadPage = useCallback(async (reset: boolean) => {
    const q = query.current;
    if (q.busy && !reset) return;
    const generation = reset ? ++q.generation : q.generation;
    q.busy = true;
    if (reset) setLoading(true);
    try {
      const offset = reset ? 0 : tracksRef.current.length;
      const page = await invoke<{ total: number; tracks: Track[] }>("library_query",
        { search: q.search, limit: PAGE, offset });
      if (generation !== q.generation) return;
      setTotal(page.total);
      setTracks((prev) => (reset ? page.tracks : [...prev, ...page.tracks]));
    } catch { /* not ready */ }
    finally {
      if (generation === q.generation) { q.busy = false; setLoading(false); }
    }
  }, []);
  const tracksRef = useRef<Track[]>([]);
  tracksRef.current = tracks;
  const loadTracks = useCallback(() => loadPage(true), [loadPage]);
  useLibraryScan(setNotice, loadTracks);

  const selectTab = useCallback((next: Tab) => {
    setTab(next);
    try {
      localStorage.setItem("riars.library.tab", next);
    } catch { /* storage not available */ }
  }, []);

  useEffect(() => { loadTracks(); }, [loadTracks]);

  // Search in the backend, once typing pauses.
  useEffect(() => {
    const id = setTimeout(() => {
      if (query.current.search === search) return;
      query.current.search = search;
      loadPage(true);
    }, 250);
    return () => clearTimeout(id);
  }, [search, loadPage]);

  const onScroll = useCallback((e: React.UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    if (el.scrollTop + el.clientHeight > el.scrollHeight - 300 && tracksRef.current.length < total) {
      loadPage(false);
    }
  }, [loadPage, total]);

  const addFolder = useCallback(async () => {
    try {
      const chosen = await open({ directory: true, multiple: true });
      if (!chosen) return;
      const paths = Array.isArray(chosen) ? chosen : [chosen];
      if (paths.length === 0) return;
      await invoke<string[]>("library_add_folders", { paths });
      setNotice(null);
    } catch (e) {
      setNotice({ text: String(e), error: true });
    }
  }, []);
  const searchInput = useRef<HTMLInputElement>(null);
  useCommand("library.addFolder", addFolder);
  // Ctrl+L from the Folders tab switches back to the list first.
  const focusSearch = useRef(false);
  useCommand("library.search", useCallback(() => {
    focusSearch.current = true;
    selectTab("library");
    searchInput.current?.focus();
  }, [selectTab]));
  useEffect(() => {
    if (tab === "library" && focusSearch.current) {
      focusSearch.current = false;
      searchInput.current?.focus();
    }
  }, [tab]);

  const insertTracks = useCallback(async (list: Track[], position: "End" | "Next") => {
    if (list.length === 0) return;
    try {
      await invoke("playlist_insert", {
        tracks: list.map((track) => ({
          path: track.file_path, title: track.title, artist: track.artist, durationMs: track.duration_ms,
        })),
        position,
      });
      window.dispatchEvent(new Event("playlist-changed"));
      setNotice(null);
    } catch (e) {
      setNotice({ text: String(e), error: true });
    }
  }, []);

  const addToPlaylist = useCallback((track: Track) => { void insertTracks([track], "End"); }, [insertTracks]);

  const prelisten = useCallback(async (track: Track) => {
    try {
      await preview.play(track.id, 0);
      setNotice({ text: t("analysis.media.prelistenStarted", { title: track.title }) });
    } catch (e) {
      setNotice({ text: String(e), error: true });
    }
  }, [t]);

  // Forget the rows that left the list (new search, removed tracks).
  useEffect(() => {
    setSelected((prev) => {
      if (prev.size === 0) return prev;
      const valid = new Set(tracks.map((tr) => tr.id));
      let changed = false;
      const next = new Set<number>();
      for (const id of prev) {
        if (valid.has(id)) next.add(id);
        else changed = true;
      }
      return changed ? next : prev;
    });
    setAnchor((prev) => (prev !== null && tracks.some((tr) => tr.id === prev) ? prev : null));
  }, [tracks]);

  const selectedTracks = () => tracks.filter((tr) => selected.has(tr.id));
  const selectedIndexes = () => tracks.map((tr, i) => (selected.has(tr.id) ? i : -1)).filter((i) => i >= 0);

  const selectOnly = (id: number) => {
    setSelected(new Set([id]));
    setAnchor(id);
  };

  const selectRange = (from: number, to: number) => {
    const set = new Set<number>();
    for (let i = Math.min(from, to); i <= Math.max(from, to); i++) set.add(tracks[i].id);
    setSelected(set);
  };

  const onRowClick = (e: React.MouseEvent, i: number) => {
    const id = tracks[i].id;
    if (e.shiftKey && anchor !== null) {
      const a = tracks.findIndex((tr) => tr.id === anchor);
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
    const id = tracks[i].id;
    if (!selected.has(id)) selectOnly(id);
    setCtx({ x: e.clientX, y: e.clientY });
  };

  const moveSelection = (dir: 1 | -1, extend: boolean) => {
    if (tracks.length === 0) return;
    const idxs = selectedIndexes();
    const current = idxs.length > 0 ? idxs[idxs.length - 1] : -1;
    const next = Math.max(0, Math.min(tracks.length - 1, current + dir));
    if (!extend) {
      selectOnly(tracks[next].id);
      return;
    }
    const a = anchor !== null ? tracks.findIndex((tr) => tr.id === anchor) : -1;
    const from = a >= 0 ? a : current >= 0 ? current : next;
    selectRange(from, next);
    setAnchor(tracks[from].id);
  };

  const openMenuAtSelection = () => {
    if (tracks.length === 0) return;
    let idxs = selectedIndexes();
    if (idxs.length === 0) {
      selectOnly(tracks[0].id);
      idxs = [0];
    }
    const row = listRef.current?.querySelector<HTMLElement>(`[data-row-index="${idxs[0]}"]`);
    const rect = row?.getBoundingClientRect();
    setCtx({
      x: rect ? rect.left + 40 : 80,
      y: rect ? rect.top + rect.height / 2 : 80,
    });
  };

  const removeFromLibrary = async () => {
    const list = selectedTracks();
    if (list.length === 0) return;
    if (!window.confirm(t("contextMenu.confirmRemoveTracks", { count: list.length }))) return;
    try {
      const removed = await invoke<number>("library_remove_tracks", { ids: list.map((tr) => tr.id) });
      await loadTracks();
      setNotice({ text: t("contextMenu.removedTracks", { count: removed }) });
      window.dispatchEvent(new Event("playlist-changed"));
    } catch (e) {
      setNotice({ text: String(e), error: true });
    }
  };

  const detectMarks = async () => {
    const ids = selectedTracks().map((tr) => tr.id);
    if (ids.length === 0) return;
    try {
      await analysis.batchStart({ Tracks: { track_ids: ids } }, true);
      setNotice({ text: t("contextMenu.analysing", { count: ids.length }) });
    } catch (e) {
      setNotice({ text: String(e), error: true });
    }
  };

  const copyPaths = async () => {
    const paths = selectedTracks().map((tr) => tr.file_path);
    if (paths.length === 0) return;
    try {
      await navigator.clipboard.writeText(paths.join("\n"));
      setNotice({ text: t("contextMenu.pathCopied") });
    } catch (e) {
      setNotice({ text: String(e), error: true });
    }
  };

  const onListKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable) return;
    const idxs = selectedIndexes();
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      moveSelection(e.key === "ArrowDown" ? 1 : -1, e.shiftKey);
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      setSelected(new Set(tracks.map((tr) => tr.id)));
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
      if (idxs.length > 0) void insertTracks(selectedTracks(), "End");
      return;
    }
    if (e.key === "Delete") {
      e.preventDefault();
      if (idxs.length > 0) void removeFromLibrary();
      return;
    }
    if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
      e.preventDefault();
      openMenuAtSelection();
    }
  };

  const handleSearch = useCallback((e: React.ChangeEvent<HTMLInputElement>) => setSearch(e.target.value), []);

  const fmt = (ms: number) => {
    const s = Math.floor(ms / 1000);
    return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
  };

  const menuTracks = selectedTracks();
  const menuTrack = menuTracks.length === 1 ? menuTracks[0] : null;
  const menuTitle = menuTrack ? menuTrack.title : t("contextMenu.selected", { count: selected.size });

  const libraryMenu = (): ContextEntry[] => [
    {
      label: t("contextMenu.addToPlaylist"), shortcut: t("contextMenu.keyEnter"), disabled: menuTracks.length === 0,
      action: () => { void insertTracks(menuTracks, "End"); },
    },
    {
      label: t("contextMenu.playNext"), disabled: menuTracks.length === 0,
      action: () => { void insertTracks(menuTracks, "Next"); },
    },
    {
      label: t("contextMenu.prelisten"), disabled: menuTrack === null,
      action: () => { if (menuTrack) void prelisten(menuTrack); },
    },
    {
      label: t("contextMenu.marks"), disabled: menuTrack === null,
      action: () => { if (menuTrack) setCueTrack(menuTrack); },
    },
    {
      label: t("contextMenu.detectMarks"), disabled: menuTracks.length === 0,
      action: () => { void detectMarks(); },
    },
    "separator",
    {
      label: t("contextMenu.openFolder"), disabled: menuTrack === null,
      action: () => {
        if (menuTrack) invoke("reveal_in_folder", { path: menuTrack.file_path }).catch((e) => setNotice({ text: String(e), error: true }));
      },
    },
    {
      label: t("contextMenu.copyPath"), disabled: menuTracks.length === 0,
      action: () => { void copyPaths(); },
    },
    "separator",
    {
      label: t("contextMenu.removeFromLibrary"), shortcut: t("contextMenu.keyDelete"), danger: true,
      disabled: menuTracks.length === 0,
      action: () => { void removeFromLibrary(); },
    },
  ];

  return (
    <div style={S.wrap}>
      <div style={S.toolbar}>
        <h2 style={S.title}>{t("library.title")}</h2>
        <div style={{ display: "flex", gap: 6 }}>
          <button style={S.btn} onClick={addFolder}>{t("library.addFolder")}</button>
          {tab === "library" && <button style={S.btn} onClick={loadTracks}>{t("library.load")}</button>}
        </div>
      </div>
      <div style={S.tabs} role="tablist">
        <button role="tab" aria-selected={tab === "library"} data-testid="library-tab-library"
          style={{ ...S.tab, ...(tab === "library" ? S.tabActive : {}) }}
          onClick={() => selectTab("library")}>
          {t("library.folders.tabLibrary")}
        </button>
        <button role="tab" aria-selected={tab === "folders"} data-testid="library-tab-folders"
          style={{ ...S.tab, ...(tab === "folders" ? S.tabActive : {}) }}
          onClick={() => selectTab("folders")}>
          {t("library.folders.tabFolders")}
        </button>
      </div>
      {tab === "library" ? (
        <>
          <div style={S.search}>
            <input ref={searchInput} style={S.input} placeholder={t("library.searchPlaceholder")} value={search} onChange={handleSearch}
              aria-label={t("library.searchPlaceholder")} />
          </div>
          {notice && (
            <div style={{ ...S.notice, color: notice.error ? "var(--red)" : "var(--text-secondary)" }}>
              {notice.text}
            </div>
          )}
          <div style={S.count} data-testid="library-count">{t("library.count", { count: total })}</div>
          <div ref={listRef} style={S.list} onScroll={onScroll} tabIndex={0} onKeyDown={onListKeyDown}
            onClick={() => listRef.current?.focus()} data-testid="library-list">
            {loading && <div style={S.empty}>{t("library.loading")}</div>}
            {!loading && tracks.length === 0 && (
              <div style={S.empty}>{t("library.empty")}</div>
            )}
            {tracks.map((tr, i) => (
              <div key={tr.id} data-row-index={i} draggable
                style={{ ...S.track, ...(selected.has(tr.id) ? S.selected : {}) }}
                title={tr.file_path}
                onClick={(e) => onRowClick(e, i)}
                onContextMenu={(e) => onRowContext(e, i)}
                onDoubleClick={() => addToPlaylist(tr)}
                onDragStart={(e) => {
                  e.dataTransfer.effectAllowed = "copy";
                  e.dataTransfer.setData("application/x-riars-track", JSON.stringify({
                    id: tr.id, title: tr.title, file_path: tr.file_path, duration_ms: tr.duration_ms,
                  }));
                }}>
                <div style={S.trackBody}>
                  <div style={S.name}>{tr.title}</div>
                  <div style={S.meta}>
                    <span>{tr.artist || t("library.unknownArtist")}</span>
                    {tr.duration_ms > 0 && <span style={{ fontFamily: "var(--font-mono)" }}>{fmt(tr.duration_ms)}</span>}
                  </div>
                </div>
                <button style={S.marks} title={t("analysis.media.marks")}
                  aria-label={t("analysis.media.marks")}
                  onClick={(e) => { e.stopPropagation(); setCueTrack(tr); }}
                  onDoubleClick={(e) => e.stopPropagation()}>✎</button>
              </div>
            ))}
          </div>
        </>
      ) : (
        <div style={S.folderPane}>
          <FolderExplorer onLibraryChanged={loadTracks} />
        </div>
      )}

      {ctx && (
        <ContextMenu x={ctx.x} y={ctx.y} title={menuTitle} items={libraryMenu()} onClose={() => setCtx(null)} />
      )}

      {cueTrack && (
        <CueEditor trackId={cueTrack.id} title={cueTrack.title} onClose={() => setCueTrack(null)} />
      )}
    </div>
  );
}
