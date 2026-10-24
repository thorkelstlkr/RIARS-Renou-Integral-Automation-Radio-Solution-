import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { PlaylistItem } from "./api";

export interface EngineState {
  state: "Stopped" | "Playing" | "Paused";
  volume: number;
  current_track: string | null;
  position_ms: number;
  duration_ms: number;
  dead_air?: boolean;
  dead_air_count?: number;
}

export interface OnAirPlaylist {
  items: PlaylistItem[];
  current_index: number;
}

const STOPPED: EngineState = { state: "Stopped", volume: 0.8, current_track: null, position_ms: 0, duration_ms: 0 };

/** The engine state and the on-air playlist, kept current from backend events. */
export function useOnAir() {
  const [engine, setEngine] = useState<EngineState>(STOPPED);
  const [playlist, setPlaylist] = useState<OnAirPlaylist>({ items: [], current_index: 0 });
  // Revision of the playlist held here: only changes are transferred.
  const revision = useRef<number | null>(null);

  const refresh = useCallback(async () => {
    try {
      const change = await invoke<{ revision: number; playlist: OnAirPlaylist } | null>(
        "playlist_changes", { since: revision.current });
      if (change) {
        revision.current = change.revision;
        setPlaylist(change.playlist);
      }
    } catch { /* backend not ready */ }
  }, []);

  useEffect(() => {
    invoke<EngineState>("engine_get_state").then(setEngine).catch(() => {});
    refresh();
    const offState = listen<EngineState>("engine-state", (e) => setEngine(e.payload));
    const offPlaylist = listen("playlist-revision", () => refresh());
    return () => {
      offState.then((f) => f());
      offPlaylist.then((f) => f());
    };
  }, [refresh]);

  return { engine, playlist };
}

/**
 * When each item from the one on air onwards should start, in ms since the epoch,
 * from the position now; null for items already played.
 */
export function startTimes(items: PlaylistItem[], current: number, engine: EngineState, now = Date.now()): (number | null)[] {
  const out: (number | null)[] = items.map(() => null);
  if (items.length === 0) return out;
  const playing = engine.state !== "Stopped";
  let t = playing ? now - engine.position_ms : now;
  for (let i = current; i < items.length; i++) {
    out[i] = t;
    t += items[i].duration_ms || 180_000;
  }
  return out;
}
