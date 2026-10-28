// Types and Tauri commands of long-term automation. The types mirror the Rust ones in
// crates/riars-core/src/automation (serde JSON); times are "HH:MM:SS", dates
// "YYYY-MM-DD", date-times "YYYY-MM-DDTHH:MM:SS" (local time).
import { invoke } from "@tauri-apps/api/core";

export type ItemKind =
  | "Music" | "Jingle" | "Bumper" | "StationId" | "Sweeper" | "Promo" | "Spot" | "Program" | "Bed" | "Other";
export const ITEM_KINDS: ItemKind[] = [
  "Music", "Jingle", "Bumper", "StationId", "Sweeper", "Promo", "Spot", "Program", "Bed", "Other",
];

export interface Separation {
  song_minutes: number;
  artist_minutes: number;
  album_minutes: number;
  horizontal: boolean;
  imaging_others: number;
}

export interface TrackFilter {
  genre?: string | null;
  artist?: string | null;
  folder?: string | null;
  year_from?: number | null;
  year_to?: number | null;
  energy_min?: number | null;
  energy_max?: number | null;
  min_duration_ms?: number | null;
  max_duration_ms?: number | null;
}

export interface Category {
  id: number; // 0 creates
  name: string;
  kind: ItemKind;
  color: string;
  parent_id: number | null;
  filter: TrackFilter | null;
  separation: Separation | null;
}

export type RuleField = "Genre" | "Artist" | "Folder";
export interface CategoryRule {
  id: number;
  category_id: number;
  field: RuleField;
  pattern: string;
}

/** A track of a category, as the generator sees it. */
export interface CategoryTrack {
  track_id: number;
  title: string;
  artist: string;
  album: string;
  duration_ms: number;
  weight: number;
  last_played_at: string | null;
  codes: string[];
}

export interface LibraryTrack {
  id: number;
  title: string;
  artist: string;
  album: string;
  genre: string;
  duration_ms: number;
  file_path: string;
}

export interface Anchor {
  minute: number;
  second: number;
  hard: boolean;
  max_wait_seconds: number;
}

export type SlotItem =
  | { Category: { category_id: number; count: number; require_codes: string[]; exclude_codes: string[] } }
  | { Imaging: { category_id: number } }
  | { AdBreak: { name: string; max_seconds: number; intro_category: number | null; outro_category: number | null } }
  | { Playlist: { playlist_id: string; count: number | null } }
  | { Track: { path: string } }
  | { Fill: { category_id: number } };
export type SlotKind = "Category" | "Imaging" | "AdBreak" | "Playlist" | "Track" | "Fill";
export const slotKind = (item: SlotItem): SlotKind => Object.keys(item)[0] as SlotKind;

export interface ClockSlot {
  at: Anchor | null;
  item: SlotItem;
}

export interface Clock {
  id: string;
  name: string;
  color: string;
  duration_minutes: number;
  slots: ClockSlot[];
  fill_category: number | null;
}

export type GridTarget = { Clock: { clock_id: string } } | { Playlist: { playlist_id: string } };

export interface GridRule {
  id: string;
  name: string;
  priority: number;
  target: GridTarget;
  /** Bit 0 = Monday ... bit 6 = Sunday. */
  weekdays: number;
  /** Bit 0 = January ... bit 11 = December. */
  months: number;
  /** Bits 0-4 = first to fifth week of the month, bit 5 = last seven days. */
  weeks_of_month: number;
  start: string;
  end: string;
  date_from: string | null;
  date_to: string | null;
  enabled: boolean;
}

export interface ImagingRules {
  jingle_category: number | null;
  jingle_every: number;
  sweeper_on_change: number | null;
}

export interface TalkOverConfig {
  enabled: boolean;
  on_intro: boolean;
  on_outro: boolean;
  margin_ms: number;
  duck_db: number;
  min_confidence: number;
  kinds: ItemKind[];
}

export interface AutomationConfig {
  enabled: boolean;
  default_clock: string | null;
  fallback_category: number | null;
  separation: Separation;
  imaging: ImagingRules;
  min_items_ahead: number;
  min_minutes_ahead: number;
  keep_played: number;
  ad_break_seconds: number;
  no_overlap_kinds: ItemKind[];
  talk_over: TalkOverConfig;
}

export interface Advertiser {
  id: number; // 0 creates
  name: string;
  contact: string;
  notes: string;
}

export interface Daypart {
  weekdays: number;
  start: string;
  end: string;
}

export type SponsorTarget = { Clock: { clock_id: string } } | { GridRule: { rule_id: string } };
export interface Sponsorship {
  target: SponsorTarget;
  opening: boolean;
  closing: boolean;
}

export interface CampaignSpot {
  track_id: number;
  weight: number;
}

export interface Campaign {
  id: number; // 0 creates
  advertiser_id: number;
  name: string;
  date_from: string;
  date_to: string;
  plays_per_day: number;
  max_per_hour: number;
  dayparts: Daypart[];
  priority: number;
  sponsorship: Sponsorship | null;
  enabled: boolean;
  spots: CampaignSpot[];
}

export interface DeliveryRow {
  date: string;
  campaign_id: number;
  campaign: string;
  advertiser_id: number;
  advertiser: string;
  target: number;
  aired: number;
  partial: number;
  seconds: number;
  missed: number;
}

export type EndReason = "Finished" | "Skipped" | "DeadAir" | "Stopped";
export interface PlayLogEntry {
  id: number;
  aired_at: string;
  ended_at: string;
  track_id: number | null;
  file_path: string;
  title: string;
  artist: string;
  kind: ItemKind;
  category_id: number | null;
  campaign_id: number | null;
  planned_at: string | null;
  played_ms: number;
  reason: EndReason;
  complete: boolean;
}

export interface PeriodInfo {
  start: string;
  end: string;
  clock_id: string | null;
  playlist_id: string | null;
  rule_id: string | null;
  slot: number;
}

export interface AutomationStatus {
  enabled: boolean;
  period: PeriodInfo | null;
  /** [date-time, message], oldest first. */
  warnings: [string, string][];
}

/** A playlist item as the engine keeps it, with the automation fields. */
export interface PlaylistItem {
  id: string;
  track_path: string;
  title: string;
  artist: string;
  duration_ms: number;
  kind?: ItemKind;
  track_id?: number | null;
  category_id?: number | null;
  campaign_id?: number | null;
  planned_at?: string | null;
  generated?: boolean;
  no_overlap?: boolean;
  /** The playlist stops when this item ends, waiting for Play or Next. */
  stop_after?: boolean;
  /** Imaging played over this item's intro or outro. */
  overlay?: { item: PlaylistItem; start_ms: number; duck_db: number } | null;
}

/** Payload of the "automation" event (same as the WebSocket message). */
export interface AutomationEvent {
  type: "automation";
  generated: number;
  logged: number;
  cut_to: string | null;
  warnings: string[];
}

export type CsvKind = "log" | "delivery" | "campaign-log";

/** Tauri commands (src-tauri/src/automation_cmds.rs). Errors reject with a message. */
export const automation = {
  status: () => invoke<AutomationStatus>("automation_status"),
  getConfig: () => invoke<AutomationConfig>("automation_get_config"),
  saveConfig: (config: AutomationConfig) => invoke<AutomationConfig>("automation_save_config", { config }),
  setEnabled: (enabled: boolean) => invoke<AutomationConfig>("automation_set_enabled", { enabled }),

  categories: () => invoke<Category[]>("automation_categories"),
  saveCategory: (category: Category) => invoke<Category>("automation_save_category", { category }),
  deleteCategory: (id: number) => invoke<void>("automation_delete_category", { id }),
  categoryTracks: (categoryId: number, trackIds: number[], weight = 1, remove = false) =>
    invoke<number>("automation_category_tracks", { categoryId, trackIds, weight, remove }),
  /** [category id, weight] pairs. */
  trackCategories: (trackId: number) => invoke<[number, number][]>("automation_track_categories", { trackId }),
  setTrackCodes: (trackId: number, codes: string[]) => invoke<string[]>("automation_set_track_codes", { trackId, codes }),
  setTrackEnabled: (trackId: number, enabled: boolean) => invoke<void>("automation_set_track_enabled", { trackId, enabled }),
  codes: () => invoke<string[]>("automation_codes"),
  /** A page of the tracks of a category (assigned, or matched by its filter). */
  categoryMembers: (categoryId: number, limit = 100, offset = 0) =>
    invoke<{ total: number; tracks: CategoryTrack[] }>("automation_category_members", { categoryId, limit, offset }),
  /** Library tracks by id (unknown ids are left out). */
  tracks: (ids: number[]) => invoke<LibraryTrack[]>("automation_tracks", { ids }),

  rules: () => invoke<CategoryRule[]>("automation_rules"),
  saveRule: (rule: CategoryRule) => invoke<number>("automation_save_rule", { rule }),
  deleteRule: (id: number) => invoke<void>("automation_delete_rule", { id }),
  applyRules: () => invoke<number>("automation_apply_rules"),

  clocks: () => invoke<Clock[]>("automation_clocks"),
  saveClock: (clock: Clock) => invoke<Clock>("automation_save_clock", { clock }),
  deleteClock: (id: string) => invoke<void>("automation_delete_clock", { id }),

  grid: () => invoke<GridRule[]>("automation_grid"),
  saveGridRule: (rule: GridRule) => invoke<GridRule>("automation_save_grid_rule", { rule }),
  deleteGridRule: (id: string) => invoke<void>("automation_delete_grid_rule", { id }),

  advertisers: () => invoke<Advertiser[]>("automation_advertisers"),
  saveAdvertiser: (advertiser: Advertiser) => invoke<Advertiser>("automation_save_advertiser", { advertiser }),
  deleteAdvertiser: (id: number) => invoke<void>("automation_delete_advertiser", { id }),
  campaigns: () => invoke<Campaign[]>("automation_campaigns"),
  saveCampaign: (campaign: Campaign) => invoke<Campaign>("automation_save_campaign", { campaign }),
  deleteCampaign: (id: number) => invoke<void>("automation_delete_campaign", { id }),

  /** What would air from `from` (default now) for `hours` (1 to 48). Writes nothing. */
  preview: (hours: number, from?: string) => invoke<PlaylistItem[]>("automation_preview", { hours, from: from ?? null }),
  log: (from?: string, to?: string, limit?: number) =>
    invoke<PlayLogEntry[]>("automation_log", { from: from ?? null, to: to ?? null, limit: limit ?? null }),
  delivery: (from?: string, to?: string) => invoke<DeliveryRow[]>("automation_delivery", { from: from ?? null, to: to ?? null }),
  /** Write a CSV to `path` (from a save dialog); resolves with the bytes written. */
  exportCsv: (kind: CsvKind, path: string, opts: { from?: string; to?: string; advertiserId?: number; campaignId?: number } = {}) =>
    invoke<number>("automation_export_csv", {
      kind, path,
      from: opts.from ?? null, to: opts.to ?? null,
      advertiserId: opts.advertiserId ?? null, campaignId: opts.campaignId ?? null,
    }),
};

/** Event name the backend emits with an `AutomationEvent`. */
export const AUTOMATION_EVENT = "automation";

export const fmtDuration = (ms: number) => {
  const s = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = (s % 60).toString().padStart(2, "0");
  return h > 0 ? `${h}:${m.toString().padStart(2, "0")}:${ss}` : `${m}:${ss}`;
};

/** "HH:MM:SS" of a date-time string. */
export const timeOf = (dateTime: string | null | undefined) => (dateTime ? dateTime.slice(11, 19) : "");

/** Today and a date-time in the backend's local format. */
export const today = () => {
  const d = new Date();
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};

/** Colour of each item kind, the same in every list and badge. */
export const KIND_COLORS: Record<ItemKind, string> = {
  Music: "var(--blue)",
  Jingle: "var(--yellow)",
  Bumper: "var(--yellow)",
  StationId: "var(--yellow)",
  Sweeper: "var(--yellow)",
  Promo: "var(--green)",
  Spot: "var(--red)",
  Program: "#a371f7",
  Bed: "var(--text-secondary)",
  Other: "var(--text-secondary)",
};
