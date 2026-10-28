// Live broadcasting: microphone, sound bank (cartwall) and play modes. Types mirror the
// Rust ones in crates/riars-core/src/live and src/audio/{engine,live}.rs (serde JSON).
import { invoke } from "@tauri-apps/api/core";

export type PlayMode = "Automatic" | "Assisted" | "Manual";
export type MicDuck = "WhileOpen" | "WhileSpeaking";
export type CartMode = "Fire" | "Toggle" | "Loop";

export const CART_PAGES = 8;
/** Buttons per page: 4 rows × 6 columns, row-major. */
export const CART_SLOTS = 24;
export const CART_COLUMNS = 6;
/** Keys for the buttons of the visible page, in slot order. */
export const CART_KEYS = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "q", "w", "e", "r", "t", "y", "u", "i", "o", "p"];

export interface MicConfig {
  /** Input card by name; null is the system default. */
  device: string | null;
  gain_db: number;          // -20..30
  high_pass: boolean;
  gate: boolean;
  gate_threshold_db: number; // -80..-10
  compressor: boolean;
  duck_db: number;           // 0..40
  duck: MicDuck;
  /** Let the API and AI agents open the microphone. */
  allow_remote: boolean;
  /** The input card plays the microphone itself: RIARS adds it only to the stream. */
  direct_monitor: boolean;
}

export interface MicStatus {
  /** The input card is open. */
  available: boolean;
  on: boolean;
  device: string | null;
  /** Loudest level since the previous read, linear (1 = full scale). */
  level: number;
  speaking: boolean;
  error: string | null;
}

export interface CartPlaying {
  /** page * CART_SLOTS + slot */
  button: number;
  /** null while it loops */
  left_ms: number | null;
}

export interface LiveStatus {
  mode: PlayMode;
  mic: MicStatus;
  carts: CartPlaying[];
}

export interface CartButton {
  page: number;
  slot: number;
  track_id: number | null;
  file_path: string;
  label: string;
  /** "" or "#rrggbb" */
  color: string;
  gain_db: number;
  mode: CartMode;
  duck: boolean;
}

export interface CartBank {
  /** 0 = new */
  id: number;
  name: string;
  buttons: CartButton[];
}

export interface CartBankSummary {
  id: number;
  name: string;
  buttons: number;
}

export const cartId = (page: number, slot: number) => page * CART_SLOTS + slot;

export const live = {
  /** Mode, microphone and what the bank plays. Poll it (~150 ms) while the On air tab is shown. */
  status: () => invoke<LiveStatus>("live_status"),
  setMode: (mode: PlayMode) => invoke<void>("live_set_mode", { mode }),

  /** Open the input card (off air) so the meter works; resolves with its name. */
  micOpen: () => invoke<string>("live_mic_open"),
  micClose: () => invoke<void>("live_mic_close"),
  /** On or off air; opens the card first if needed. */
  setMic: (on: boolean) => invoke<void>("live_set_mic", { on }),
  micConfig: () => invoke<MicConfig>("live_mic_config"),
  setMicConfig: (config: MicConfig) => invoke<void>("live_set_mic_config", { config }),
  /** Cards that can record, and the system default. */
  inputDevices: () => invoke<{ devices: string[]; default: string | null }>("live_input_devices"),

  banks: () => invoke<CartBankSummary[]>("live_banks"),
  bank: (id: number) => invoke<CartBank | null>("live_bank", { id }),
  /** Insert (id 0) or replace a bank; resolves with its id. */
  saveBank: (bank: CartBank) => invoke<number>("live_save_bank", { bank }),
  deleteBank: (id: number) => invoke<void>("live_delete_bank", { id }),
  activeBank: () => invoke<number | null>("live_active_bank"),
  setActiveBank: (id: number | null) => invoke<void>("live_set_active_bank", { id }),
  setButton: (bankId: number, button: CartButton) => invoke<void>("live_set_button", { bankId, button }),
  clearButton: (bankId: number, page: number, slot: number) => invoke<void>("live_clear_button", { bankId, page, slot }),
  /** Decode the short sounds of a page ahead so the first press is instant. */
  preload: (paths: string[]) => invoke<void>("live_preload", { paths }),
  /** Press a button; resolves true when it started, false when it stopped (Toggle/Loop). */
  fire: (button: CartButton) => invoke<boolean>("live_fire", { button }),
  /** Fade out one button's sounds, or all with null. */
  stop: (button: number | null) => invoke<void>("live_stop", { button }),

  /** Stop when this playlist item ends. */
  setStopAfter: (itemId: string, value: boolean) => invoke<void>("playlist_set_stop_after", { itemId, value }),
};
