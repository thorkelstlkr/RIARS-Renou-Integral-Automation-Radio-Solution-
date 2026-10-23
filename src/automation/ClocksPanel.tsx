import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { automation, slotKind } from "./api";
import type { Anchor, Category, Clock, ClockSlot, SlotItem, SlotKind } from "./api";
import ClockPie from "./ClockPie";

interface SavedPlaylist {
  id: string;
  name: string;
  items: number;
  duration_ms: number;
}

const KINDS: SlotKind[] = ["Category", "Imaging", "AdBreak", "Playlist", "Track", "Fill"];
const ANCHOR_RE = /^(\d{1,3}):([0-5]?\d)$/;

const pad2 = (n: number) => n.toString().padStart(2, "0");
const anchorText = (at: Anchor | null) => (at ? `${pad2(at.minute)}:${pad2(at.second)}` : "");
const clone = <T,>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

const slugify = (name: string) =>
  name
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "") || "clock";

const makeId = (name: string, clocks: Clock[]) => {
  const base = slugify(name);
  if (!clocks.some((clock) => clock.id === base)) return base;
  let id = base;
  do {
    id = `${base}-${Math.random().toString(36).slice(2, 6)}`;
  } while (clocks.some((clock) => clock.id === id));
  return id;
};

const defaultItem = (kind: SlotKind, categoryId: number, playlistId: string): SlotItem => {
  switch (kind) {
    case "Category":
      return { Category: { category_id: categoryId, count: 1, require_codes: [], exclude_codes: [] } };
    case "Imaging":
      return { Imaging: { category_id: categoryId } };
    case "AdBreak":
      return { AdBreak: { name: "", max_seconds: 180, intro_category: null, outro_category: null } };
    case "Playlist":
      return { Playlist: { playlist_id: playlistId, count: null } };
    case "Track":
      return { Track: { path: "" } };
    case "Fill":
      return { Fill: { category_id: categoryId } };
  }
};

const S: Record<string, React.CSSProperties> = {
  panel: { display: "flex", flexDirection: "column", height: "100%" },
  header: {
    display: "flex", alignItems: "center", gap: 8, padding: "10px 16px",
    borderBottom: "1px solid var(--border)",
  },
  title: { fontSize: 14, fontWeight: 600, flex: 1 },
  body: { flex: 1, display: "flex", minHeight: 0 },
  side: {
    width: 250, flexShrink: 0, display: "flex", flexDirection: "column",
    borderRight: "1px solid var(--border)", background: "var(--bg-secondary)",
  },
  sideToolbar: {
    display: "flex", gap: 6, padding: "8px 10px", borderBottom: "1px solid var(--border)",
  },
  list: { flex: 1, overflowY: "auto" },
  empty: { padding: "24px 16px", textAlign: "center", color: "var(--text-muted)", fontSize: 13 },
  clockRow: {
    display: "flex", alignItems: "center", gap: 8, width: "100%", textAlign: "left",
    padding: "8px 10px", borderBottom: "1px solid rgba(255,255,255,0.03)",
  },
  active: { background: "var(--bg-panel)", borderLeft: "3px solid var(--accent)" },
  dot: { width: 10, height: 10, borderRadius: 5, flexShrink: 0 },
  clockName: { flex: 1, fontSize: 13, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  clockLen: { fontSize: 11, color: "var(--text-muted)", fontFamily: "var(--font-mono)" },
  main: { flex: 1, minWidth: 0, display: "flex", flexDirection: "column", overflow: "auto", padding: 12 },
  editorHead: { display: "flex", flexWrap: "wrap", alignItems: "flex-end", gap: 10 },
  field: { display: "flex", flexDirection: "column", gap: 3, fontSize: 11, color: "var(--text-secondary)" },
  actions: { display: "flex", gap: 6, marginLeft: "auto" },
  input: { fontSize: 12, padding: "3px 6px" },
  btn: {
    padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12,
    background: "var(--bg-panel)", border: "1px solid var(--border)",
  },
  btnPrimary: {
    padding: "4px 12px", borderRadius: "var(--radius)", fontSize: 12,
    background: "var(--accent)", border: "1px solid var(--accent)", color: "#fff",
  },
  error: { padding: "6px 16px", fontSize: 12, color: "var(--red)", borderBottom: "1px solid var(--border)" },
  slotsHeader: {
    display: "flex", alignItems: "center", gap: 8, marginTop: 16, marginBottom: 6,
  },
  slotsTitle: { flex: 1, fontSize: 12, fontWeight: 600, textTransform: "uppercase", letterSpacing: 0.5, color: "var(--text-muted)" },
  slotsBody: { display: "flex", gap: 16, alignItems: "flex-start" },
  tableWrap: { flex: 1, overflowX: "auto", minWidth: 0 },
  table: { width: "100%", borderCollapse: "collapse", fontSize: 12 },
  th: {
    textAlign: "left", padding: "4px 6px", fontSize: 10, fontWeight: 600,
    textTransform: "uppercase", letterSpacing: 0.5, color: "var(--text-muted)",
    borderBottom: "1px solid var(--border)",
  },
  td: { padding: "4px 6px", borderBottom: "1px solid rgba(255,255,255,0.04)", verticalAlign: "top" },
  timeRow: { display: "flex", alignItems: "center", gap: 4, flexWrap: "wrap" },
  miniCheck: { display: "flex", alignItems: "center", gap: 3, fontSize: 10, color: "var(--text-secondary)" },
  details: { display: "flex", alignItems: "center", gap: 4, flexWrap: "wrap" },
  icon: {
    padding: "0 5px", fontSize: 12, background: "transparent", color: "var(--text-muted)",
  },
  pie: { flexShrink: 0, width: 232, display: "flex", justifyContent: "center" },
  noSlots: { padding: "12px 6px", color: "var(--text-muted)", fontSize: 12 },
};

function CodesField({
  value, label, onCommit,
}: { value: string[]; label: string; onCommit: (codes: string[]) => void }) {
  return (
    <input
      key={value.join(",")}
      defaultValue={value.join(", ")}
      aria-label={label}
      title={label}
      placeholder={label}
      style={{ ...S.input, width: 110 }}
      onBlur={(e) => {
        const codes = e.currentTarget.value.split(",").map((code) => code.trim()).filter(Boolean);
        e.currentTarget.value = codes.join(", ");
        onCommit(codes);
      }}
    />
  );
}

export default function ClocksPanel() {
  const { t } = useTranslation();
  const [clocks, setClocks] = useState<Clock[]>([]);
  const [categories, setCategories] = useState<Category[]>([]);
  const [playlists, setPlaylists] = useState<SavedPlaylist[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [draft, setDraft] = useState<Clock | null>(null);
  const [original, setOriginal] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (): Promise<Clock[]> => {
    try {
      const [list, cats] = await Promise.all([automation.clocks(), automation.categories()]);
      setClocks(list);
      setCategories(cats);
      setError(null);
      return list;
    } catch (e) {
      setError(String(e));
      return [];
    }
  }, []);

  useEffect(() => {
    load().then((list) => {
      if (list.length > 0) {
        setSelected(list[0].id);
        setDraft(clone(list[0]));
        setOriginal(JSON.stringify(list[0]));
      }
    });
    invoke<SavedPlaylist[]>("playlists_list")
      .then(setPlaylists)
      .catch((e) => setError(String(e)));
  }, [load]);

  const dirty = draft != null && (draft.id === "" || JSON.stringify(draft) !== original);

  const selectClock = (id: string) => {
    if (draft?.id === id) return;
    if (dirty && !window.confirm(t("automation.clocks.confirmDiscard"))) return;
    const clock = clocks.find((c) => c.id === id);
    if (!clock) return;
    setSelected(id);
    setDraft(clone(clock));
    setOriginal(JSON.stringify(clock));
    setError(null);
  };

  const newClock = () => {
    if (dirty && !window.confirm(t("automation.clocks.confirmDiscard"))) return;
    setSelected(null);
    setDraft({
      id: "", name: t("automation.clocks.defaultName"), color: "#58a6ff",
      duration_minutes: 60, slots: [], fill_category: null,
    });
    setOriginal(null);
    setError(null);
  };

  const removeClock = async () => {
    if (!draft) return;
    if (draft.id && !window.confirm(t("automation.clocks.confirmDelete", { name: draft.name || draft.id }))) return;
    try {
      if (draft.id) await automation.deleteClock(draft.id);
      const list = await load();
      const next = list.find((clock) => clock.id !== draft.id) ?? null;
      setSelected(next?.id ?? null);
      setDraft(next ? clone(next) : null);
      setOriginal(next ? JSON.stringify(next) : null);
    } catch (e) {
      setError(String(e));
    }
  };

  const save = async () => {
    if (!draft) return;
    const clock: Clock = { ...draft, id: draft.id || makeId(draft.name, clocks) };
    try {
      const saved = await automation.saveClock(clock);
      await load();
      setSelected(saved.id);
      setDraft(clone(saved));
      setOriginal(JSON.stringify(saved));
    } catch (e) {
      setError(String(e));
    }
  };

  const discard = () => {
    if (!draft) return;
    if (!draft.id) {
      setSelected(null);
      setDraft(null);
      setOriginal(null);
      setError(null);
      return;
    }
    const source = clocks.find((clock) => clock.id === draft.id);
    if (source) {
      setDraft(clone(source));
      setOriginal(JSON.stringify(source));
    }
    setError(null);
  };

  const patchSlot = (index: number, patch: (slot: ClockSlot) => ClockSlot) => {
    if (!draft) return;
    setDraft({
      ...draft,
      slots: draft.slots.map((slot, i) => (i === index ? patch(slot) : slot)),
    });
  };

  const setItem = (index: number, item: SlotItem) => {
    patchSlot(index, (slot) => ({ ...slot, item }));
  };

  const setAnchorText = (index: number, text: string, input: HTMLInputElement) => {
    if (!draft) return;
    const slot = draft.slots[index];
    if (!text.trim()) {
      patchSlot(index, (s) => ({ ...s, at: null }));
      input.value = "";
      return;
    }
    const match = text.trim().match(ANCHOR_RE);
    if (!match) {
      input.value = anchorText(slot.at);
      return;
    }
    const minute = Number(match[1]);
    const second = Number(match[2]);
    patchSlot(index, (s) => ({
      ...s,
      at: {
        minute,
        second,
        hard: s.at?.hard ?? false,
        max_wait_seconds: s.at?.max_wait_seconds ?? 0,
      },
    }));
    input.value = anchorText({ minute, second, hard: false, max_wait_seconds: 0 });
  };

  const moveSlot = (index: number, delta: number) => {
    if (!draft) return;
    const target = index + delta;
    if (target < 0 || target >= draft.slots.length) return;
    const slots = draft.slots.slice();
    [slots[index], slots[target]] = [slots[target], slots[index]];
    setDraft({ ...draft, slots });
  };

  const removeSlot = (index: number) => {
    if (!draft) return;
    setDraft({ ...draft, slots: draft.slots.filter((_, i) => i !== index) });
  };

  const addSlot = () => {
    if (!draft) return;
    const item = defaultItem("Category", categories[0]?.id ?? 0, playlists[0]?.id ?? "");
    setDraft({ ...draft, slots: [...draft.slots, { at: null, item }] });
  };

  const categoryOptions = categories.map((category) => (
    <option key={category.id} value={category.id}>{category.name}</option>
  ));

  const renderDetails = (index: number, slot: ClockSlot) => {
    const item = slot.item;
    if ("Category" in item) {
      return (
        <div style={S.details}>
          <select
            value={item.Category.category_id}
            aria-label={t("automation.clocks.category")}
            style={S.input}
            onChange={(e) => setItem(index, {
              Category: { ...item.Category, category_id: Number(e.target.value) },
            })}
          >
            {categoryOptions}
          </select>
          <input
            type="number" min={1} value={item.Category.count}
            aria-label={t("automation.clocks.count")} title={t("automation.clocks.count")}
            style={{ ...S.input, width: 52 }}
            onChange={(e) => setItem(index, {
              Category: { ...item.Category, count: Number(e.target.value) },
            })}
          />
          <CodesField
            value={item.Category.require_codes}
            label={t("automation.clocks.requireCodes")}
            onCommit={(codes) => setItem(index, {
              Category: { ...item.Category, require_codes: codes },
            })}
          />
          <CodesField
            value={item.Category.exclude_codes}
            label={t("automation.clocks.excludeCodes")}
            onCommit={(codes) => setItem(index, {
              Category: { ...item.Category, exclude_codes: codes },
            })}
          />
        </div>
      );
    }
    if ("Imaging" in item) {
      return (
        <div style={S.details}>
          <select
            value={item.Imaging.category_id}
            aria-label={t("automation.clocks.category")}
            style={S.input}
            onChange={(e) => setItem(index, {
              Imaging: { ...item.Imaging, category_id: Number(e.target.value) },
            })}
          >
            {categoryOptions}
          </select>
        </div>
      );
    }
    if ("Fill" in item) {
      return (
        <div style={S.details}>
          <select
            value={item.Fill.category_id}
            aria-label={t("automation.clocks.category")}
            style={S.input}
            onChange={(e) => setItem(index, {
              Fill: { ...item.Fill, category_id: Number(e.target.value) },
            })}
          >
            {categoryOptions}
          </select>
        </div>
      );
    }
    if ("AdBreak" in item) {
      return (
        <div style={S.details}>
          <input
            value={item.AdBreak.name}
            aria-label={t("automation.clocks.name")}
            placeholder={t("automation.clocks.name")}
            style={{ ...S.input, width: 110 }}
            onChange={(e) => setItem(index, {
              AdBreak: { ...item.AdBreak, name: e.target.value },
            })}
          />
          <input
            type="number" min={0} value={item.AdBreak.max_seconds}
            aria-label={t("automation.clocks.maxSeconds")} title={t("automation.clocks.maxSeconds")}
            style={{ ...S.input, width: 60 }}
            onChange={(e) => setItem(index, {
              AdBreak: { ...item.AdBreak, max_seconds: Number(e.target.value) },
            })}
          />
          <select
            value={item.AdBreak.intro_category ?? ""}
            aria-label={t("automation.clocks.introCategory")} title={t("automation.clocks.introCategory")}
            style={S.input}
            onChange={(e) => setItem(index, {
              AdBreak: {
                ...item.AdBreak,
                intro_category: e.target.value === "" ? null : Number(e.target.value),
              },
            })}
          >
            <option value="">{t("automation.clocks.introCategory")}</option>
            {categoryOptions}
          </select>
          <select
            value={item.AdBreak.outro_category ?? ""}
            aria-label={t("automation.clocks.outroCategory")} title={t("automation.clocks.outroCategory")}
            style={S.input}
            onChange={(e) => setItem(index, {
              AdBreak: {
                ...item.AdBreak,
                outro_category: e.target.value === "" ? null : Number(e.target.value),
              },
            })}
          >
            <option value="">{t("automation.clocks.outroCategory")}</option>
            {categoryOptions}
          </select>
        </div>
      );
    }
    if ("Playlist" in item) {
      return (
        <div style={S.details}>
          <select
            value={item.Playlist.playlist_id}
            aria-label={t("automation.clocks.playlist")}
            style={S.input}
            onChange={(e) => setItem(index, {
              Playlist: { ...item.Playlist, playlist_id: e.target.value },
            })}
          >
            {playlists.length === 0 && <option value="">{t("automation.clocks.none")}</option>}
            {playlists.map((playlist) => (
              <option key={playlist.id} value={playlist.id}>{playlist.name}</option>
            ))}
          </select>
          <input
            type="number" min={1}
            value={item.Playlist.count ?? ""}
            placeholder={t("automation.clocks.all")}
            aria-label={t("automation.clocks.count")} title={t("automation.clocks.count")}
            style={{ ...S.input, width: 58 }}
            onChange={(e) => setItem(index, {
              Playlist: {
                ...item.Playlist,
                count: e.target.value === "" ? null : Number(e.target.value),
              },
            })}
          />
        </div>
      );
    }
    return (
      <div style={S.details}>
        <input
          value={item.Track.path}
          aria-label={t("automation.clocks.trackPath")}
          placeholder={t("automation.clocks.trackPath")}
          style={{ ...S.input, width: 240 }}
          onChange={(e) => setItem(index, { Track: { path: e.target.value } })}
        />
      </div>
    );
  };

  return (
    <div style={S.panel}>
      <div style={S.header}>
        <span style={S.title}>{t("automation.clocks.title")}</span>
      </div>
      {error && <div style={S.error}>{error}</div>}
      <div style={S.body}>
        <div style={S.side}>
          <div style={S.sideToolbar}>
            <button type="button" style={S.btn} onClick={newClock}>
              {t("automation.clocks.new")}
            </button>
            <button type="button" style={S.btn} onClick={removeClock} disabled={!draft}>
              {t("automation.clocks.delete")}
            </button>
          </div>
          <div style={S.list}>
            {clocks.length === 0 && <div style={S.empty}>{t("automation.clocks.empty")}</div>}
            {clocks.map((clock) => (
              <button
                key={clock.id}
                type="button"
                style={{ ...S.clockRow, ...(selected === clock.id ? S.active : {}) }}
                onClick={() => selectClock(clock.id)}
              >
                <span style={{ ...S.dot, background: clock.color || "var(--text-muted)" }} />
                <span style={S.clockName}>{clock.name}</span>
                <span style={S.clockLen}>
                  {t("automation.clocks.minutes", { count: clock.duration_minutes })}
                </span>
              </button>
            ))}
            {draft && draft.id === "" && (
              <div style={{ ...S.clockRow, ...S.active }}>
                <span style={{ ...S.dot, background: draft.color || "var(--text-muted)" }} />
                <span style={S.clockName}>{draft.name}</span>
                <span style={S.clockLen}>
                  {t("automation.clocks.minutes", { count: draft.duration_minutes })}
                </span>
              </div>
            )}
          </div>
        </div>

        <div style={S.main}>
          {!draft ? (
            <div style={S.empty}>{t("automation.clocks.selectPrompt")}</div>
          ) : (
            <>
              <div style={S.editorHead}>
                <label style={S.field}>
                  {t("automation.clocks.name")}
                  <input
                    value={draft.name}
                    style={{ ...S.input, width: 180 }}
                    onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                  />
                </label>
                <label style={S.field}>
                  {t("automation.clocks.color")}
                  <input
                    type="color"
                    value={/^#[0-9a-f]{6}$/i.test(draft.color) ? draft.color : "#58a6ff"}
                    style={{ ...S.input, width: 42, padding: 2 }}
                    onChange={(e) => setDraft({ ...draft, color: e.target.value })}
                  />
                </label>
                <label style={S.field}>
                  {t("automation.clocks.length")}
                  <input
                    type="number" min={5} max={1440}
                    value={draft.duration_minutes}
                    style={{ ...S.input, width: 72 }}
                    onChange={(e) => setDraft({ ...draft, duration_minutes: Number(e.target.value) })}
                  />
                </label>
                <label style={S.field}>
                  {t("automation.clocks.fillCategory")}
                  <select
                    value={draft.fill_category ?? ""}
                    style={S.input}
                    onChange={(e) => setDraft({
                      ...draft,
                      fill_category: e.target.value === "" ? null : Number(e.target.value),
                    })}
                  >
                    <option value="">{t("automation.clocks.none")}</option>
                    {categoryOptions}
                  </select>
                </label>
                <div style={S.actions}>
                  <button type="button" style={S.btnPrimary} onClick={save}>
                    {t("automation.clocks.save")}
                  </button>
                  <button type="button" style={S.btn} onClick={discard} disabled={!dirty}>
                    {t("automation.clocks.discard")}
                  </button>
                </div>
              </div>

              <div style={S.slotsHeader}>
                <span style={S.slotsTitle}>{t("automation.clocks.slots")}</span>
                <button type="button" style={S.btn} onClick={addSlot}>
                  {t("automation.clocks.addSlot")}
                </button>
              </div>

              <div style={S.slotsBody}>
                <div style={S.tableWrap}>
                  {draft.slots.length === 0 ? (
                    <div style={S.noSlots}>{t("automation.clocks.noSlots")}</div>
                  ) : (
                    <table style={S.table}>
                      <thead>
                        <tr>
                          <th style={S.th}>{t("automation.clocks.time")}</th>
                          <th style={S.th}>{t("automation.clocks.type")}</th>
                          <th style={S.th}>{t("automation.clocks.details")}</th>
                          <th style={S.th}>{t("automation.clocks.actions")}</th>
                        </tr>
                      </thead>
                      <tbody>
                        {draft.slots.map((slot, index) => (
                          <tr key={index}>
                            <td style={S.td}>
                              <div style={S.timeRow}>
                                <input
                                  key={`at-${index}-${slot.at ? `${slot.at.minute}:${slot.at.second}` : "none"}`}
                                  defaultValue={anchorText(slot.at)}
                                  placeholder="mm:ss"
                                  aria-label={t("automation.clocks.time")}
                                  title={t("automation.clocks.anchor")}
                                  style={{ ...S.input, width: 56, fontFamily: "var(--font-mono)" }}
                                  onBlur={(e) => setAnchorText(index, e.currentTarget.value, e.currentTarget)}
                                />
                                <label style={S.miniCheck} title={t("automation.clocks.hard")}>
                                  <input
                                    type="checkbox"
                                    checked={!!slot.at?.hard}
                                    disabled={!slot.at}
                                    onChange={(e) => {
                                      const hard = e.target.checked;
                                      patchSlot(index, (s) => (s.at ? { ...s, at: { ...s.at, hard } } : s));
                                    }}
                                  />
                                  {t("automation.clocks.hard")}
                                </label>
                                <input
                                  type="number" min={0}
                                  value={slot.at?.max_wait_seconds ?? 0}
                                  disabled={!slot.at}
                                  aria-label={t("automation.clocks.maxWait")}
                                  title={t("automation.clocks.maxWait")}
                                  style={{ ...S.input, width: 54 }}
                                  onChange={(e) => {
                                    const seconds = Number(e.target.value);
                                    patchSlot(index, (s) => (s.at
                                      ? { ...s, at: { ...s.at, max_wait_seconds: seconds } }
                                      : s));
                                  }}
                                />
                              </div>
                            </td>
                            <td style={S.td}>
                              <select
                                value={slotKind(slot.item)}
                                aria-label={t("automation.clocks.type")}
                                style={S.input}
                                onChange={(e) => setItem(
                                  index,
                                  defaultItem(
                                    e.target.value as SlotKind,
                                    categories[0]?.id ?? 0,
                                    playlists[0]?.id ?? "",
                                  ),
                                )}
                              >
                                {KINDS.map((kind) => (
                                  <option key={kind} value={kind}>
                                    {t(`automation.clocks.kind.${kind.toLowerCase()}`)}
                                  </option>
                                ))}
                              </select>
                            </td>
                            <td style={S.td}>{renderDetails(index, slot)}</td>
                            <td style={S.td}>
                              <div style={S.timeRow}>
                                <button
                                  type="button" style={S.icon}
                                  title={t("automation.clocks.moveUp")}
                                  disabled={index === 0}
                                  onClick={() => moveSlot(index, -1)}
                                >
                                  ↑
                                </button>
                                <button
                                  type="button" style={S.icon}
                                  title={t("automation.clocks.moveDown")}
                                  disabled={index === draft.slots.length - 1}
                                  onClick={() => moveSlot(index, 1)}
                                >
                                  ↓
                                </button>
                                <button
                                  type="button" style={S.icon}
                                  title={t("automation.clocks.remove")}
                                  onClick={() => removeSlot(index)}
                                >
                                  ✕
                                </button>
                              </div>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  )}
                </div>
                <div style={S.pie}>
                  <ClockPie clock={draft} categories={categories} />
                </div>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
