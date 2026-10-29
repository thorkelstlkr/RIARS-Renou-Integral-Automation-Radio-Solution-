import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useTrackDrop } from "./trackDrag";

interface DropProgress {
  job: number;
  state: "listing" | "reading";
  done: number;
  total: number;
}

interface DropResult {
  added: number;
  skipped: number;
  failed: number;
  truncated: boolean;
}

export interface DropNotice {
  text: string;
  error?: boolean;
}

export interface PlaylistDrop {
  /** Where the insertion line is: before the row at this index, or after the last row
   *  when it equals the number of rows. `null` when nothing is being dragged over the list. */
  dropAt: number | null;
  /** Progress line of the drops being added, or the result of the last one. */
  notice: DropNotice | null;
}

let nextJob = 1;

/**
 * Files and folders dropped on the playlist from the system's file manager (the
 * webview's drag-drop event), and tracks dragged from the library. They are added
 * before the row under the pointer; the backend walks the folders, adds unknown
 * files to the library and reports progress as `playlist-drop` events.
 *
 * `listRef` is the scrolling list of rows (`data-row-index` on each row); `itemIds`
 * the ids of the rows, in order; `tab` the playlist tab shown (null: the list on air).
 */
export function usePlaylistDrop(
  listRef: React.RefObject<HTMLDivElement | null>,
  itemIds: string[],
  tab: string | null = null,
): PlaylistDrop {
  const { t } = useTranslation();
  const [dropAt, setDropAt] = useState<number | null>(null);
  const [progress, setProgress] = useState<Record<number, DropProgress>>({});
  const [result, setResult] = useState<DropNotice | null>(null);
  const ids = useRef(itemIds);
  ids.current = itemIds;
  const tabRef = useRef(tab);
  tabRef.current = tab;
  const tr = useRef(t);
  tr.current = t;

  /** The insertion index for a point in CSS pixels, or null when it is outside the list. */
  const indexAt = useCallback((x: number, y: number): number | null => {
    const list = listRef.current;
    if (!list) return null;
    const box = list.getBoundingClientRect();
    if (x < box.left || x > box.right || y < box.top || y > box.bottom) return null;
    const row = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-row-index]");
    if (!row || !list.contains(row)) return ids.current.length;
    const index = Number(row.dataset.rowIndex);
    const rect = row.getBoundingClientRect();
    return y < rect.top + rect.height / 2 ? index : index + 1;
  }, [listRef]);

  /** Scroll the list while the pointer is near its top or bottom edge. */
  const autoScroll = useCallback((y: number) => {
    const list = listRef.current;
    if (!list) return;
    const box = list.getBoundingClientRect();
    if (y < box.top + 32) list.scrollTop -= 24;
    else if (y > box.bottom - 32) list.scrollTop += 24;
  }, [listRef]);

  const add = useCallback(async (paths: string[], at: number) => {
    if (paths.length === 0) return;
    const job = nextJob++;
    const beforeId = ids.current[at] ?? null;
    setResult(null);
    setProgress((p) => ({ ...p, [job]: { job, state: "listing", done: 0, total: 0 } }));
    try {
      const r = await invoke<DropResult>("playlist_drop_files", { paths, beforeId, tab: tabRef.current, job });
      const t = tr.current;
      const parts: string[] = [];
      if (r.added > 0) parts.push(t("playlist.drop.added", { count: r.added }));
      if (r.skipped > 0) parts.push(t("playlist.drop.skipped", { count: r.skipped }));
      if (r.failed > 0) parts.push(t("playlist.drop.failed", { count: r.failed }));
      if (r.truncated) parts.push(t("playlist.drop.truncated"));
      setResult(r.added === 0 && r.failed === 0
        ? { text: t("playlist.drop.nothing"), error: true }
        : { text: parts.join(" · "), error: r.failed > 0 });
      window.dispatchEvent(new Event("playlist-changed"));
      if (r.added > 0) window.dispatchEvent(new Event("library-changed"));
    } catch (e) {
      setResult({ text: String(e), error: true });
    } finally {
      setProgress((p) => {
        const next = { ...p };
        delete next[job];
        return next;
      });
    }
  }, []);

  // The result of a drop stays on screen for a while.
  useEffect(() => {
    if (!result) return;
    const timer = window.setTimeout(() => setResult(null), result.error ? 10000 : 5000);
    return () => window.clearTimeout(timer);
  }, [result]);

  useEffect(() => {
    const off = listen<DropProgress>("playlist-drop", (e) => {
      const event = e.payload;
      // Only the drops of this window, still running.
      setProgress((p) => (p[event.job] ? { ...p, [event.job]: event } : p));
    });
    return () => { off.then((f) => f()); };
  }, []);

  // Files and folders from the system's file manager. Positions come in physical pixels.
  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | null = null;
    getCurrentWebview().onDragDropEvent((e) => {
      const event = e.payload;
      if (event.type === "leave") {
        setDropAt(null);
        return;
      }
      const ratio = window.devicePixelRatio || 1;
      const x = event.position.x / ratio;
      const y = event.position.y / ratio;
      const at = indexAt(x, y);
      if (event.type === "drop") {
        setDropAt(null);
        if (at !== null) void add(event.paths, at);
        return;
      }
      if (at !== null) autoScroll(y);
      setDropAt(at);
    }).then((f) => {
      if (alive) unlisten = f;
      else f();
    }).catch(() => { /* not running in Tauri */ });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [indexAt, autoScroll, add]);

  // Tracks dragged from the library or the Folders tab (pointer drag, see trackDrag.ts).
  useTrackDrop((e) => {
    if (e.phase === "end") { setDropAt(null); return; }
    const at = indexAt(e.x, e.y);
    if (e.phase === "drop") {
      setDropAt(null);
      if (at !== null) void add(e.tracks.map((tr) => tr.file_path), at);
      return;
    }
    if (at !== null) autoScroll(e.y);
    setDropAt(at);
  });

  const running = Object.values(progress);
  let notice: DropNotice | null = result;
  if (running.length > 0) {
    const reading = running.filter((p) => p.state === "reading");
    const done = reading.reduce((sum, p) => sum + p.done, 0);
    const total = reading.reduce((sum, p) => sum + p.total, 0);
    notice = total === 0
      ? { text: t("playlist.drop.listing") }
      : { text: t("playlist.drop.reading", { done, total }) };
  }

  return { dropAt, notice };
}
