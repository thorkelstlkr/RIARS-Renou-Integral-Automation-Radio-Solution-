import { useState, useCallback, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

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
  search: { padding: "8px 12px", borderBottom: "1px solid var(--border)" },
  input: { width: "100%", fontSize: 13 },
  notice: { padding: "6px 12px", fontSize: 12, borderBottom: "1px solid var(--border)", wordBreak: "break-word" },
  list: { flex: 1, overflowY: "auto" },
  count: { padding: "4px 12px", fontSize: 11, color: "var(--text-muted)", borderBottom: "1px solid var(--border)" },
  empty: { padding: "24px 12px", textAlign: "center", color: "var(--text-muted)", fontSize: 13 },
  track: { padding: "8px 12px", cursor: "pointer", borderBottom: "1px solid rgba(255,255,255,0.03)" },
  name: { fontSize: 13, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  meta: { display: "flex", justifyContent: "space-between", fontSize: 11, color: "var(--text-muted)", marginTop: 2 },
};

/// Tracks per page: the list grows as it is scrolled, never the whole library at once.
const PAGE = 100;

export default function MediaLibrary() {
  const { t } = useTranslation();
  const [tracks, setTracks] = useState<Track[]>([]);
  const [total, setTotal] = useState(0);
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(false);
  const [notice, setNotice] = useState<{ text: string; error?: boolean } | null>(null);
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
    const folder = await open({ directory: true, multiple: false });
    if (typeof folder !== "string") return;
    setNotice({ text: t("library.scanning") });
    try {
      const count = await invoke<number>("library_scan_folder", { path: folder });
      setNotice({ text: t("library.added", { count }) });
      setSearch("");
      await loadTracks();
    } catch (e) {
      setNotice({ text: String(e), error: true });
    }
  }, [loadTracks, t]);

  const addToPlaylist = useCallback(async (track: Track) => {
    try {
      await invoke("playlist_add", {
        path: track.file_path, title: track.title, artist: track.artist, durationMs: track.duration_ms,
      });
      window.dispatchEvent(new Event("playlist-changed"));
      setNotice(null);
    } catch (e) {
      setNotice({ text: String(e), error: true });
    }
  }, []);

  const handleSearch = useCallback((e: React.ChangeEvent<HTMLInputElement>) => setSearch(e.target.value), []);

  const fmt = (ms: number) => {
    const s = Math.floor(ms / 1000);
    return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
  };

  return (
    <div style={S.wrap}>
      <div style={S.toolbar}>
        <h2 style={S.title}>{t("library.title")}</h2>
        <div style={{ display: "flex", gap: 6 }}>
          <button style={S.btn} onClick={addFolder}>{t("library.addFolder")}</button>
          <button style={S.btn} onClick={loadTracks}>{t("library.load")}</button>
        </div>
      </div>
      <div style={S.search}>
        <input style={S.input} placeholder={t("library.searchPlaceholder")} value={search} onChange={handleSearch} />
      </div>
      {notice && (
        <div style={{ ...S.notice, color: notice.error ? "var(--red)" : "var(--text-secondary)" }}>
          {notice.text}
        </div>
      )}
      <div style={S.count} data-testid="library-count">{t("library.count", { count: total })}</div>
      <div style={S.list} onScroll={onScroll} data-testid="library-list">
        {loading && <div style={S.empty}>{t("library.loading")}</div>}
        {!loading && tracks.length === 0 && (
          <div style={S.empty}>{t("library.empty")}</div>
        )}
        {tracks.map((tr) => (
          <div key={tr.id} style={S.track}
            title={tr.file_path}
            onDoubleClick={() => addToPlaylist(tr)}>
            <div style={S.name}>{tr.title}</div>
            <div style={S.meta}>
              <span>{tr.artist || t("library.unknownArtist")}</span>
              {tr.duration_ms > 0 && <span style={{ fontFamily: "var(--font-mono)" }}>{fmt(tr.duration_ms)}</span>}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
