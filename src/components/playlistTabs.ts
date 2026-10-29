import { invoke } from "@tauri-apps/api/core";
import type { PlaylistItem } from "../automation/api";

/** A playlist tab, without its items (see riars-core `PlaylistTab`). */
export interface PlaylistTab {
  id: string;
  name: string;
  /** Planned start, "HH:MM:SS", for the Start column of a list not on air. */
  start_time: string | null;
  items: number;
  duration_ms: number;
}

export interface Tabs {
  tabs: PlaylistTab[];
  /** Id of the tab whose list is on air (the only one that plays). */
  on_air: string;
}

export interface TabList {
  id: string;
  name: string;
  items: PlaylistItem[];
  current_index: number;
}

export type CopyTo = "End" | "Next";

export const tabsApi = {
  list: () => invoke<Tabs>("playlist_tabs"),
  get: (id: string) => invoke<TabList>("playlist_tab_get", { id }),
  open: (name: string, fromSaved?: string) => invoke<TabList>("playlist_tab_new", { name, fromSaved: fromSaved ?? null }),
  close: (id: string) => invoke<Tabs>("playlist_tab_close", { id }),
  rename: (id: string, name: string) => invoke<Tabs>("playlist_tab_rename", { id, name }),
  setStart: (id: string, start: string | null) => invoke<Tabs>("playlist_tab_start", { id, start }),
  onAir: (id: string) => invoke<Tabs>("playlist_tab_on_air", { id }),
  /** Copy (or move) items between lists; `null` is the list on air. */
  copy: (ids: string[], from: string | null, to: string | null, position: CopyTo, remove: boolean) =>
    invoke<number>("playlist_tab_copy", { ids, from, to, position, remove }),
};

// The tab shown in the playlist panel, so "Add to playlist" in the library and the
// Folders tab adds to the list being edited. `null` while it is the list on air.
let shown: string | null = null;
export const shownTab = () => shown;
export const setShownTab = (id: string | null) => { shown = id; };

/** Start times (ms since the epoch) of a list planned to start at "HH:MM:SS" today. */
export function plannedStarts(items: { duration_ms: number }[], start: string | null): (number | null)[] {
  if (!start) return items.map(() => null);
  const [h, m, s] = start.split(":").map(Number);
  const base = new Date();
  base.setHours(h, m, s || 0, 0);
  let at = base.getTime();
  return items.map((item) => {
    const begins = at;
    at += item.duration_ms;
    return begins;
  });
}
