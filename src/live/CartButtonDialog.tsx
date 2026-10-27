import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import Dialog from "../menu/Dialog";
import { live, type CartButton, type CartMode } from "./api";

interface Track {
  id: number;
  title: string;
  artist: string;
  duration_ms: number;
  file_path: string;
}

const COLORS = ["#e94560", "#f78c26", "#d29922", "#3fb950", "#2ea7b3", "#58a6ff", "#a371f7", "#8b949e"];

const S: Record<string, React.CSSProperties> = {
  field: { display: "flex", flexDirection: "column" as const, gap: 4, marginBottom: 12 },
  label: { fontSize: 12, color: "var(--text-secondary)" },
  input: { fontSize: 13, width: "100%" },
  row: { display: "flex", alignItems: "center", gap: 8 },
  range: { flex: 1 },
  value: { width: 64, textAlign: "right" as const, fontFamily: "var(--font-mono)", fontSize: 12 },
  swatches: { display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" as const },
  swatch: { width: 24, height: 24, borderRadius: 5, border: "2px solid transparent", padding: 0 },
  swatchOn: { borderColor: "var(--text-primary)" },
  none: {
    width: 24, height: 24, borderRadius: 5, border: "1px solid var(--border)", background: "var(--bg-panel)",
    fontSize: 11, color: "var(--text-muted)", padding: 0,
  },
  file: {
    fontSize: 12, color: "var(--text-secondary)", fontFamily: "var(--font-mono)", padding: "6px 8px",
    border: "1px solid var(--border)", borderRadius: "var(--radius)", background: "var(--bg-input)",
    overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const,
  },
  results: {
    marginTop: 6, maxHeight: 160, overflowY: "auto" as const, border: "1px solid var(--border)",
    borderRadius: "var(--radius)",
  },
  result: {
    display: "block", width: "100%", textAlign: "left" as const, padding: "6px 8px",
    background: "transparent", borderBottom: "1px solid rgba(255,255,255,0.04)", fontSize: 12,
  },
  resultMeta: { color: "var(--text-muted)", fontSize: 11 },
  empty: { padding: "6px 8px", fontSize: 12, color: "var(--text-muted)" },
  check: { display: "flex", alignItems: "center", gap: 8, fontSize: 13, marginBottom: 12 },
  error: { color: "var(--red)", fontSize: 12, marginTop: 6, wordBreak: "break-word" as const },
  actions: { display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 8 },
  btn: { padding: "6px 14px", borderRadius: "var(--radius)", background: "var(--bg-panel)", fontSize: 13 },
  primary: { padding: "6px 14px", borderRadius: "var(--radius)", background: "var(--accent)", color: "#fff", fontSize: 13 },
};

/** Assign or edit one button of the sound bank. */
export default function CartButtonDialog({ bankId, page, slot, button, onClose, onSaved }: {
  bankId: number;
  page: number;
  slot: number;
  button?: CartButton | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState<CartButton>(() => button ?? {
    page, slot, track_id: null, file_path: "", label: "", color: "", gain_db: 0, mode: "Fire", duck: false,
  });
  const [search, setSearch] = useState("");
  const [results, setResults] = useState<Track[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const text = search.trim();
    if (!text) {
      setResults([]);
      return;
    }
    let alive = true;
    const id = setTimeout(() => {
      invoke<{ total: number; tracks: Track[] }>("library_query", { search: text, limit: 20, offset: 0 })
        .then((pageResult) => { if (alive) setResults(pageResult.tracks); })
        .catch((e) => { if (alive) setError(String(e)); });
    }, 250);
    return () => { alive = false; clearTimeout(id); };
  }, [search]);

  const patch = (values: Partial<CartButton>) => setDraft((prev) => ({ ...prev, ...values }));

  const choose = (track: Track) => {
    patch({ track_id: track.id, file_path: track.file_path, label: track.title.slice(0, 40) });
    setSearch("");
    setResults([]);
  };

  const save = async () => {
    const label = draft.label.trim();
    if (!label) {
      setError(t("live.labelRequired"));
      return;
    }
    if (!draft.file_path) {
      setError(t("live.fileRequired"));
      return;
    }
    try {
      await live.setButton(bankId, { ...draft, label: label.slice(0, 40) });
      onSaved();
      onClose();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <Dialog title={t("live.buttonEditor")} width={520} onClose={onClose}>
      <div style={S.field}>
        <span style={S.label}>{t("live.label")}</span>
        <input style={S.input} value={draft.label} maxLength={40} aria-label={t("live.label")}
          onChange={(e) => patch({ label: e.target.value })} />
      </div>

      <div style={S.field}>
        <span style={S.label}>{t("live.color")}</span>
        <div style={S.swatches}>
          <button type="button" title={t("live.noColor")} aria-label={t("live.noColor")}
            style={{ ...S.none, ...(draft.color === "" ? S.swatchOn : {}) }}
            onClick={() => patch({ color: "" })}>—</button>
          {COLORS.map((color) => (
            <button key={color} type="button" title={color} aria-label={color}
              style={{
                ...S.swatch, background: color,
                ...(draft.color.toLowerCase() === color ? S.swatchOn : {}),
              }}
              onClick={() => patch({ color })} />
          ))}
        </div>
      </div>

      <div style={S.field}>
        <span style={S.label}>{t("live.gain")}</span>
        <div style={S.row}>
          <input style={S.range} type="range" min={-30} max={12} step={1}
            aria-label={t("live.gain")} value={draft.gain_db}
            onChange={(e) => patch({ gain_db: Number(e.target.value) })} />
          <span style={S.value}>{`${draft.gain_db > 0 ? "+" : ""}${draft.gain_db.toFixed(0)} dB`}</span>
        </div>
      </div>

      <div style={S.field}>
        <span style={S.label}>{t("live.buttonMode")}</span>
        <select style={S.input} value={draft.mode} aria-label={t("live.buttonMode")}
          onChange={(e) => patch({ mode: e.target.value as CartMode })}>
          <option value="Fire">{t("live.cartMode.Fire")}</option>
          <option value="Toggle">{t("live.cartMode.Toggle")}</option>
          <option value="Loop">{t("live.cartMode.Loop")}</option>
        </select>
      </div>

      <label style={S.check}>
        <input type="checkbox" checked={draft.duck} onChange={(e) => patch({ duck: e.target.checked })} />
        {t("live.duck")}
      </label>

      <div style={S.field}>
        <span style={S.label}>{t("live.file")}</span>
        <div style={S.file} title={draft.file_path}>{draft.file_path || "—"}</div>
      </div>

      <div style={S.field}>
        <span style={S.label}>{t("live.chooseTrack")}</span>
        <input style={S.input} value={search} aria-label={t("live.chooseTrack")}
          placeholder={t("live.chooseTrack")} onChange={(e) => setSearch(e.target.value)} />
        {search.trim() !== "" && (
          <div style={S.results}>
            {results.length === 0 && <div style={S.empty}>{t("library.empty")}</div>}
            {results.map((track) => (
              <button key={track.id} type="button" style={S.result} onClick={() => choose(track)}>
                <div>{track.title}</div>
                <div style={S.resultMeta}>{track.artist || t("library.unknownArtist")}</div>
              </button>
            ))}
          </div>
        )}
      </div>

      {error && <div style={S.error}>{error}</div>}

      <div style={S.actions}>
        <button style={S.btn} onClick={onClose}>{t("menu.cancel")}</button>
        <button style={S.primary} onClick={save}>{t("menu.save")}</button>
      </div>
    </Dialog>
  );
}
