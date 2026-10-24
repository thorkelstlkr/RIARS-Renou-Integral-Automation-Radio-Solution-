import { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { fmtDuration } from "./api";

export interface PickedTrack {
  id: number;
  title: string;
  artist: string;
  duration_ms: number;
}

interface Track extends PickedTrack {
  file_path: string;
}

const PAGE = 100;

const S: Record<string, React.CSSProperties> = {
  panel: {
    display: "flex", flexDirection: "column", minHeight: 220, maxHeight: 320,
    border: "1px solid var(--border)", borderRadius: "var(--radius)",
    background: "var(--bg-secondary)", overflow: "hidden",
  },
  search: { padding: "6px 8px", borderBottom: "1px solid var(--border)" },
  input: { width: "100%", fontSize: 12 },
  count: {
    padding: "4px 10px", fontSize: 11, color: "var(--text-muted)",
    borderBottom: "1px solid var(--border)",
  },
  list: { flex: 1, overflowY: "auto", minHeight: 0 },
  empty: { padding: "18px 10px", textAlign: "center", color: "var(--text-muted)", fontSize: 12 },
  row: {
    display: "flex", alignItems: "center", gap: 8, padding: "4px 10px", cursor: "pointer",
    borderBottom: "1px solid rgba(255,255,255,0.03)", fontSize: 12,
  },
  rowActive: { background: "var(--bg-panel)" },
  title: { flex: 2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  artist: {
    flex: 1, color: "var(--text-secondary)", overflow: "hidden", textOverflow: "ellipsis",
    whiteSpace: "nowrap" as const,
  },
  dur: {
    width: 52, textAlign: "right" as const, fontFamily: "var(--font-mono)",
    fontSize: 11, color: "var(--text-secondary)",
  },
  foot: {
    display: "flex", alignItems: "center", gap: 8, padding: "6px 8px",
    borderTop: "1px solid var(--border)",
  },
  btn: { padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12, background: "var(--bg-panel)" },
  more: { flex: 1, padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12, background: "var(--bg-panel)" },
  error: { padding: "4px 10px", fontSize: 11, color: "var(--red)" },
};

export default function TrackPicker({
  onPick,
  multi,
}: {
  onPick: (tracks: { id: number; title: string; artist: string; duration_ms: number }[]) => void;
  multi?: boolean;
}) {
  const { t } = useTranslation();
  const [search, setSearch] = useState("");
  const [tracks, setTracks] = useState<Track[]>([]);
  const [total, setTotal] = useState(0);
  const [selected, setSelected] = useState<Set<number>>(() => new Set());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const tracksRef = useRef<Track[]>([]);
  tracksRef.current = tracks;
  const query = useRef({ search: "", busy: false, generation: 0 });

  const loadPage = useCallback(async (reset: boolean) => {
    const q = query.current;
    if (!reset && q.busy) return;
    const generation = reset ? ++q.generation : q.generation;
    q.busy = true;
    setLoading(true);
    try {
      const offset = reset ? 0 : tracksRef.current.length;
      const page = await invoke<{ total: number; tracks: Track[] }>("library_query", {
        search: q.search, limit: PAGE, offset,
      });
      if (generation !== q.generation) return;
      setTotal(page.total);
      setTracks((prev) => (reset ? page.tracks : [...prev, ...page.tracks]));
      setError(null);
    } catch (e) {
      if (generation === q.generation) setError(String(e));
    } finally {
      if (generation === q.generation) {
        q.busy = false;
        setLoading(false);
      }
    }
  }, []);

  useEffect(() => { loadPage(true); }, [loadPage]);

  useEffect(() => {
    const id = setTimeout(() => {
      if (query.current.search === search) return;
      query.current.search = search;
      setSelected(new Set());
      loadPage(true);
    }, 250);
    return () => clearTimeout(id);
  }, [search, loadPage]);

  const toggle = useCallback((id: number) => {
    setSelected((prev) => {
      if (prev.has(id)) {
        const next = new Set(prev);
        next.delete(id);
        return next;
      }
      const next = multi ? new Set(prev) : new Set<number>();
      next.add(id);
      return next;
    });
  }, [multi]);

  const add = () => {
    const picked = tracks
      .filter((tr) => selected.has(tr.id))
      .map((tr) => ({ id: tr.id, title: tr.title, artist: tr.artist, duration_ms: tr.duration_ms }));
    if (picked.length === 0) return;
    onPick(picked);
    setSelected(new Set());
  };

  return (
    <div style={S.panel}>
      <div style={S.search}>
        <input
          style={S.input}
          type="search"
          aria-label={t("automation.trackPicker.search")}
          placeholder={t("automation.trackPicker.placeholder")}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </div>
      <div style={S.count}>
        {total === 0
          ? t("automation.trackPicker.empty")
          : t("automation.trackPicker.showing", { shown: tracks.length, total })}
        {selected.size > 0 && (
          <> · {t("automation.trackPicker.selected", { count: selected.size })}</>
        )}
      </div>
      {error && <div style={S.error}>{error}</div>}
      <div style={S.list}>
        {tracks.length === 0 && <div style={S.empty}>{t("automation.trackPicker.empty")}</div>}
        {tracks.map((tr) => {
          const active = selected.has(tr.id);
          return (
            <div
              key={tr.id}
              style={{ ...S.row, ...(active ? S.rowActive : {}) }}
              title={tr.file_path}
              onClick={() => toggle(tr.id)}
            >
              {multi && (
                <input
                  type="checkbox"
                  checked={active}
                  aria-label={tr.title}
                  onClick={(e) => e.stopPropagation()}
                  onChange={() => toggle(tr.id)}
                />
              )}
              {!multi && <span style={{ width: 12, color: "var(--accent)" }}>{active ? "●" : ""}</span>}
              <span style={S.title}>{tr.title}</span>
              <span style={S.artist}>{tr.artist}</span>
              <span style={S.dur}>{fmtDuration(tr.duration_ms)}</span>
            </div>
          );
        })}
      </div>
      <div style={S.foot}>
        {tracks.length < total ? (
          <button style={S.more} onClick={() => loadPage(false)} disabled={loading}>
            {loading ? t("automation.trackPicker.loading") : t("automation.trackPicker.more")}
          </button>
        ) : (
          <span style={{ flex: 1 }} />
        )}
        <button style={S.btn} onClick={add} disabled={selected.size === 0}>
          {selected.size === 0
            ? t("automation.trackPicker.addNone")
            : t("automation.trackPicker.add", { count: selected.size })}
        </button>
      </div>
    </div>
  );
}
