// Types and pure helpers of the "Complete metadata" review: which fields a proposal
// changes, which are ticked at first, and what to send to `metadata_apply`.
import { invoke } from "@tauri-apps/api/core";

export type Field = "title" | "artist" | "album" | "genre" | "year";
export const FIELDS: Field[] = ["title", "artist", "album", "genre", "year"];

export interface TrackMetadata {
  id: number;
  title: string;
  artist: string;
  album: string;
  genre: string;
  year: number | null;
  duration_ms: number;
  file_path: string;
  source: string;
  cover_path: string | null;
}

export interface MetadataFields {
  title?: string | null;
  artist?: string | null;
  album?: string | null;
  genre?: string | null;
  year?: number | null;
}

export interface Found {
  fields: MetadataFields;
  suggested: string[];
  recording_id: string;
  release_group_id: string | null;
  cover_url: string | null;
  method: "tags" | "file_name" | "fingerprint";
  confidence: number;
  source: string;
}

export interface Proposal {
  track_id: number;
  current: TrackMetadata;
  found: Found | null;
  error: string | null;
  applied: boolean;
}

export interface LookupStatus {
  running: boolean;
  done: number;
  total: number;
  current: string | null;
  results: Proposal[];
  stopped: string | null;
  fingerprints: boolean;
}

export interface ApplyItem {
  track_id: number;
  fields: MetadataFields;
  cover_url: string | null;
  source: string;
  write_file: boolean;
}

export interface ApplyOutcome {
  track_id: number;
  applied: boolean;
  cover_saved: boolean;
  wrote_file: boolean;
  error: string | null;
  track: TrackMetadata | null;
}

/** One field a proposal would change. */
export interface FieldChange {
  field: Field;
  current: string;
  proposed: string;
}

const show = (v: string | number | null | undefined) => (v === null || v === undefined ? "" : String(v));

/** Fields where the catalogue has a value different from the current one. */
export function changes(p: Proposal): FieldChange[] {
  if (!p.found) return [];
  const out: FieldChange[] = [];
  for (const field of FIELDS) {
    const proposed = show(p.found.fields[field]);
    const current = show(p.current[field]);
    if (proposed !== "" && proposed !== current) out.push({ field, current, proposed });
  }
  return out;
}

/** Ticked fields per track: `${trackId}:${field}`, plus `${trackId}:cover`. */
export type Ticks = Set<string>;

export const tickKey = (trackId: number, field: Field | "cover") => `${trackId}:${field}`;

/** What is ticked at first: only the gaps the catalogue fills, and the cover when the
 * track has none. Values the track already has are never ticked for the operator. */
export function initialTicks(results: Proposal[]): Ticks {
  const ticks: Ticks = new Set();
  for (const p of results) {
    if (!p.found || p.applied) continue;
    const changed = new Set(changes(p).map((c) => c.field));
    for (const field of p.found.suggested) {
      if (changed.has(field as Field)) ticks.add(tickKey(p.track_id, field as Field));
    }
    if (p.found.cover_url && !p.current.cover_path) ticks.add(tickKey(p.track_id, "cover"));
  }
  return ticks;
}

/** The ticked changes, one item per track that has any. */
export function applyItems(results: Proposal[], ticks: Ticks, writeFile: boolean): ApplyItem[] {
  const items: ApplyItem[] = [];
  for (const p of results) {
    if (!p.found || p.applied) continue;
    const fields: MetadataFields = {};
    let any = false;
    for (const c of changes(p)) {
      if (!ticks.has(tickKey(p.track_id, c.field))) continue;
      any = true;
      if (c.field === "year") fields.year = Number(c.proposed);
      else fields[c.field] = c.proposed;
    }
    const cover = p.found.cover_url && ticks.has(tickKey(p.track_id, "cover")) ? p.found.cover_url : null;
    if (!any && !cover) continue;
    items.push({ track_id: p.track_id, fields, cover_url: cover, source: p.found.source, write_file: writeFile });
  }
  return items;
}

/** The small cover (250 px) for the preview. */
export const thumbnail = (coverUrl: string) => coverUrl.replace(/front-500$/, "front-250");

export const metadata = {
  start: (trackIds: number[]) => invoke<number>("metadata_lookup_start", { trackIds }),
  status: () => invoke<LookupStatus>("metadata_lookup_status"),
  cancel: () => invoke<void>("metadata_lookup_cancel"),
  apply: (items: ApplyItem[]) => invoke<ApplyOutcome[]>("metadata_apply", { items }),
  getKey: () => invoke<string | null>("metadata_settings_get"),
  setKey: (acoustidKey: string) => invoke<string | null>("metadata_settings_set", { acoustidKey }),
  missingCount: () => invoke<number>("metadata_missing_count"),
};
