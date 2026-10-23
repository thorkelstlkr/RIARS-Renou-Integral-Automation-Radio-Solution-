import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

interface PlaylistItem {
  id: string;
  track_path: string;
  title: string;
  artist: string;
  duration_ms: number;
}

interface Playlist {
  items: PlaylistItem[];
  current_index: number;
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
  tit: { flex: 2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  art: { flex: 1, color: "var(--text-secondary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  dur: { width: 70, textAlign: "right" as const, fontFamily: "var(--font-mono)", fontSize: 12, color: "var(--text-secondary)" },
  act: { width: 72, display: "flex", justifyContent: "flex-end", gap: 2 },
  icon: { padding: "0 6px", fontSize: 12, background: "transparent", color: "var(--text-muted)" },
  foot: { padding: "6px 16px", fontSize: 12, color: "var(--text-muted)", borderTop: "1px solid var(--border)" },
};

const fmt = (ms: number) => {
  if (ms <= 0) return "--:--";
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
};

export default function PlaylistPanel() {
  const { t } = useTranslation();
  const [playlist, setPlaylist] = useState<Playlist>({ items: [], current_index: 0 });
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try { setPlaylist(await invoke<Playlist>("playlist_get")); } catch { /* not ready */ }
  }, []);

  // Poll so auto-advance and additions from the library show up.
  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 1000);
    window.addEventListener("playlist-changed", refresh);
    return () => { clearInterval(id); window.removeEventListener("playlist-changed", refresh); };
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

  const { items, current_index } = playlist;
  const total = items.reduce((sum, i) => sum + i.duration_ms, 0);

  return (
    <div style={S.panel}>
      <div style={S.toolbar}>
        <h2 style={S.title}>{t("playlist.title")}</h2>
        <button style={S.btn} onClick={() => run("playlist_clear")} disabled={items.length === 0}>
          {t("playlist.clear")}
        </button>
      </div>
      <div style={S.hdr}>
        <span style={S.num}>{t("playlist.headerNumber")}</span>
        <span style={S.tit}>{t("playlist.headerTitle")}</span>
        <span style={S.art}>{t("playlist.headerArtist")}</span>
        <span style={S.dur}>{t("playlist.headerDuration")}</span>
        <span style={S.act} />
      </div>
      {error && <div style={{ ...S.foot, color: "var(--red)", borderTop: "none" }}>{error}</div>}
      <div style={S.list}>
        {items.length === 0 && <div style={S.empty}>{t("playlist.empty")}</div>}
        {items.map((item, i) => (
          <div key={item.id} title={item.track_path}
            style={{ ...S.row, ...(i === current_index ? S.active : {}) }}
            onDoubleClick={() => run("playlist_play", { index: i })}>
            <span style={S.num}>{i + 1}</span>
            <span style={S.tit}>{item.title}</span>
            <span style={S.art}>{item.artist || t("library.unknownArtist")}</span>
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
        ))}
      </div>
      <div style={S.foot}>
        {t("playlist.itemCount", { count: items.length })} · {fmt(total)}
      </div>
    </div>
  );
}
