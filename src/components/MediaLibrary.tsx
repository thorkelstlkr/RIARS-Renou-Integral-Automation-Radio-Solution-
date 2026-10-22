import { useState, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

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
  list: { flex: 1, overflowY: "auto" },
  empty: { padding: "24px 12px", textAlign: "center", color: "var(--text-muted)", fontSize: 13 },
  track: { padding: "8px 12px", cursor: "pointer", borderBottom: "1px solid rgba(255,255,255,0.03)" },
  name: { fontSize: 13, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  meta: { display: "flex", justifyContent: "space-between", fontSize: 11, color: "var(--text-muted)", marginTop: 2 },
};

export default function MediaLibrary() {
  const { t } = useTranslation();
  const [tracks, setTracks] = useState<Track[]>([]);
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(false);

  const loadTracks = useCallback(async () => {
    setLoading(true);
    try {
      setTracks(await invoke<Track[]>("library_get_tracks"));
    } catch { /* not ready */ }
    finally { setLoading(false); }
  }, []);

  const handleSearch = useCallback(async (e: React.ChangeEvent<HTMLInputElement>) => {
    const q = e.target.value;
    setSearch(q);
    if (q.length < 2) { loadTracks(); return; }
    try { setTracks(await invoke<Track[]>("library_search", { query: q })); } catch { /**/ }
  }, [loadTracks]);

  const fmt = (ms: number) => {
    const s = Math.floor(ms / 1000);
    return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
  };

  return (
    <div style={S.wrap}>
      <div style={S.toolbar}>
        <h2 style={S.title}>{t("library.title")}</h2>
        <button style={S.btn} onClick={loadTracks}>{t("library.load")}</button>
      </div>
      <div style={S.search}>
        <input style={S.input} placeholder={t("library.searchPlaceholder")} value={search} onChange={handleSearch} />
      </div>
      <div style={S.list}>
        {loading && <div style={S.empty}>{t("library.loading")}</div>}
        {!loading && tracks.length === 0 && (
          <div style={S.empty}>{t("library.empty")}</div>
        )}
        {tracks.map((tr) => (
          <div key={tr.id} style={S.track}
            onDoubleClick={() => invoke("engine_load_track", { path: tr.file_path })}>
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
