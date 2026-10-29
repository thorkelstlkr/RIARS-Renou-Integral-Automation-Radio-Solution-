import { useEffect, useRef } from "react";

/** A library or Folders row being dragged inside the window. */
export interface DraggedTrack {
  id: number | null;
  title: string;
  file_path: string;
  duration_ms: number;
}

export interface TrackDragEvent {
  /** `move` while the pointer moves, `drop` when it is released, `end` after a drop or a cancel. */
  phase: "move" | "drop" | "end";
  tracks: DraggedTrack[];
  /** Pointer position in CSS pixels. */
  x: number;
  y: number;
}

const EVENT = "riars-track-drag";
/** Pixels the pointer must travel before a press becomes a drag, so clicks still work. */
const THRESHOLD = 5;

function emit(detail: TrackDragEvent) {
  window.dispatchEvent(new CustomEvent<TrackDragEvent>(EVENT, { detail }));
}

/**
 * Pointer-driven drag of tracks between panels. HTML5 drag and drop cannot be used:
 * on Windows the webview keeps drags for itself while file drops from the system's
 * file manager are enabled (`dragDropEnabled`), so `dragover`/`drop` never fire.
 *
 * `get` returns the tracks to drag when the press turns into a drag (the row, or the
 * selection the row belongs to).
 */
export function startTrackDrag(e: React.PointerEvent, get: () => DraggedTrack[]) {
  if (e.button !== 0) return;
  const startX = e.clientX;
  const startY = e.clientY;
  let tracks: DraggedTrack[] | null = null;
  let ghost: HTMLDivElement | null = null;

  const finish = () => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("keydown", onKey, true);
    ghost?.remove();
    document.body.style.removeProperty("user-select");
    document.body.style.removeProperty("cursor");
  };

  const onMove = (ev: PointerEvent) => {
    if (!tracks) {
      if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < THRESHOLD) return;
      tracks = get();
      if (tracks.length === 0) { finish(); return; }
      window.getSelection()?.removeAllRanges();
      document.body.style.userSelect = "none";
      document.body.style.cursor = "copy";
      ghost = document.createElement("div");
      ghost.textContent = tracks.length > 1 ? `${tracks[0].title} +${tracks.length - 1}` : tracks[0].title;
      Object.assign(ghost.style, {
        position: "fixed", zIndex: "10000", pointerEvents: "none", maxWidth: "280px",
        padding: "4px 8px", borderRadius: "4px", fontSize: "12px", whiteSpace: "nowrap",
        overflow: "hidden", textOverflow: "ellipsis", opacity: "0.9",
        background: "var(--bg-panel, #222)", color: "var(--text, #eee)",
        border: "1px solid var(--accent, #c33)", boxShadow: "0 2px 8px rgba(0,0,0,.4)",
      });
      document.body.appendChild(ghost);
    }
    ghost!.style.left = `${ev.clientX + 12}px`;
    ghost!.style.top = `${ev.clientY + 12}px`;
    emit({ phase: "move", tracks, x: ev.clientX, y: ev.clientY });
  };

  const onUp = (ev: PointerEvent) => {
    finish();
    if (!tracks) return;
    // The release ends a drag, not a click on whatever is under the pointer.
    // The click (if any) follows pointerup in the same task, so the guard goes after it.
    const swallow = (c: MouseEvent) => { c.stopPropagation(); c.preventDefault(); };
    window.addEventListener("click", swallow, { capture: true, once: true });
    window.setTimeout(() => window.removeEventListener("click", swallow, { capture: true }), 0);
    emit({ phase: "drop", tracks, x: ev.clientX, y: ev.clientY });
    emit({ phase: "end", tracks, x: ev.clientX, y: ev.clientY });
  };

  const onKey = (ev: KeyboardEvent) => {
    if (ev.key !== "Escape" || !tracks) return;
    ev.preventDefault();
    ev.stopPropagation();
    const t = tracks;
    tracks = null;
    finish();
    emit({ phase: "end", tracks: t, x: -1, y: -1 });
  };

  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
  window.addEventListener("keydown", onKey, true);
}

/** Calls `handler` for every in-window track drag event. */
export function useTrackDrop(handler: (e: TrackDragEvent) => void) {
  const ref = useRef(handler);
  ref.current = handler;
  useEffect(() => {
    const on = (e: Event) => ref.current((e as CustomEvent<TrackDragEvent>).detail);
    window.addEventListener(EVENT, on);
    return () => window.removeEventListener(EVENT, on);
  }, []);
}
