import { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useCommand } from "../commands";
import { automation, fmtDuration, ITEM_KINDS, KIND_COLORS, today } from "./api";
import type {
  AutomationConfig, Category, CategoryRule, CategoryTrack, Clock, ImagingRules, ItemKind,
  RuleField, Separation, TrackFilter,
} from "./api";
import TrackPicker from "./TrackPicker";
import type { PickedTrack } from "./TrackPicker";

export function KindBadge({ kind }: { kind: ItemKind }) {
  const { t } = useTranslation();
  const color = KIND_COLORS[kind] ?? "var(--text-secondary)";
  return (
    <span
      style={{
        display: "inline-block", padding: "0 5px", borderRadius: 3,
        border: `1px solid ${color}`, color, fontSize: 10, fontWeight: 700,
        lineHeight: "15px", whiteSpace: "nowrap",
      }}
    >
      {t(`automation.kinds.${kind}`, kind)}
    </span>
  );
}

interface TreeNode {
  cat: Category;
  children: TreeNode[];
}

const buildTree = (cats: Category[], parent: number | null): TreeNode[] =>
  cats.filter((c) => c.parent_id === parent).map((cat) => ({ cat, children: buildTree(cats, cat.id) }));

const flattenTree = (nodes: TreeNode[], depth = 0): { cat: Category; depth: number }[] =>
  nodes.flatMap((n) => [{ cat: n.cat, depth }, ...flattenTree(n.children, depth + 1)]);

const descendants = (id: number, cats: Category[]): Set<number> => {
  const out = new Set<number>();
  const walk = (parent: number) => {
    for (const c of cats) {
      if (c.parent_id === parent) {
        out.add(c.id);
        walk(c.id);
      }
    }
  };
  walk(id);
  return out;
};

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

const blankCategory = (): Category => ({
  id: 0, name: "", kind: "Music", color: "#e94560", parent_id: null, filter: null, separation: null,
});

const blankSeparation = (): Separation => ({
  song_minutes: 0, artist_minutes: 0, album_minutes: 0, horizontal: false, imaging_others: 0,
});

const blankFilter = (): TrackFilter => ({
  genre: null, artist: null, folder: null, year_from: null, year_to: null,
  energy_min: null, energy_max: null, min_duration_ms: null, max_duration_ms: null,
});

const num = (value: string, fallback = 0) => {
  const n = Number(value);
  return Number.isFinite(n) ? n : fallback;
};

const optNum = (value: string): number | null => (value.trim() === "" ? null : num(value));
const secToMs = (value: string): number | null => (value.trim() === "" ? null : Math.round(num(value) * 1000));
const hexColor = (color: string) => (/^#[0-9a-f]{6}$/i.test(color) ? color : "#e94560");

const MEMBERS_PAGE = 100;

const lastPlayed = (value: string | null) => {
  if (!value) return "—";
  return value.slice(0, 10) === today() ? value.slice(11, 16) : value.slice(0, 10);
};

const S: Record<string, React.CSSProperties> = {
  panel: { display: "flex", flexDirection: "column", height: "100%" },
  toolbar: {
    display: "flex", alignItems: "center", gap: 8, padding: "8px 16px",
    borderBottom: "1px solid var(--border)",
  },
  title: { fontSize: 14, fontWeight: 600, flex: 1 },
  btn: { padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12, background: "var(--bg-panel)" },
  body: { flex: 1, display: "flex", minHeight: 0 },
  left: {
    width: 280, minWidth: 220, borderRight: "1px solid var(--border)",
    display: "flex", flexDirection: "column", minHeight: 0,
  },
  list: { flex: 1, overflowY: "auto" },
  empty: { padding: "20px 12px", textAlign: "center", color: "var(--text-muted)", fontSize: 12 },
  row: {
    display: "flex", alignItems: "center", gap: 6, padding: "6px 10px", cursor: "pointer",
    borderBottom: "1px solid rgba(255,255,255,0.03)", fontSize: 13,
  },
  rowActive: { background: "var(--bg-panel)", borderLeft: "3px solid var(--accent)" },
  swatch: { width: 10, height: 10, borderRadius: 2, flex: "0 0 auto" },
  rowName: { flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  editor: {
    flex: 1, overflowY: "auto", padding: "12px 16px", display: "flex",
    flexDirection: "column", gap: 12, minHeight: 0,
  },
  section: { border: "1px solid var(--border)", borderRadius: "var(--radius)", padding: "10px 12px" },
  sectionTitle: {
    fontSize: 11, fontWeight: 700, textTransform: "uppercase" as const, letterSpacing: 0.5,
    color: "var(--text-muted)", marginBottom: 8,
  },
  grid: { display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(150px, 1fr))", gap: 8 },
  field: { display: "flex", flexDirection: "column", gap: 3, fontSize: 11, color: "var(--text-secondary)" },
  input: { fontSize: 13 },
  check: { display: "flex", alignItems: "center", gap: 6, fontSize: 12, margin: "6px 0" },
  hint: { fontSize: 11, color: "var(--text-muted)" },
  notice: { fontSize: 12, color: "var(--green)" },
  error: { fontSize: 12, color: "var(--red)" },
  noticeBar: { padding: "6px 16px", fontSize: 12, borderBottom: "1px solid var(--border)" },
  line: { display: "flex", alignItems: "center", gap: 6, marginBottom: 4, fontSize: 12 },
  lineMain: { flex: 1 },
  actions: { display: "flex", gap: 8, alignItems: "flex-end", marginTop: 8 },
  settings: { borderTop: "1px solid var(--border)" },
  summary: {
    padding: "8px 16px", cursor: "pointer", fontSize: 12, fontWeight: 600,
    color: "var(--text-secondary)",
  },
  settingsBody: {
    padding: "10px 16px 16px", maxHeight: 340, overflowY: "auto", display: "flex",
    flexDirection: "column", gap: 10,
  },
  chips: { display: "flex", flexWrap: "wrap" as const, gap: 10 },
  tableWrap: { marginTop: 8, overflowX: "auto" as const },
  table: { width: "100%", borderCollapse: "collapse" as const, fontSize: 12 },
  th: {
    textAlign: "left" as const, padding: "4px 8px", fontSize: 10, fontWeight: 700,
    textTransform: "uppercase" as const, letterSpacing: 0.5, color: "var(--text-muted)",
    borderBottom: "1px solid var(--border)",
  },
  thCheck: { width: 26 },
  td: { padding: "3px 8px", borderBottom: "1px solid rgba(255,255,255,0.04)" },
  tdMono: {
    padding: "3px 8px", borderBottom: "1px solid rgba(255,255,255,0.04)",
    fontFamily: "var(--font-mono)", fontSize: 11,
  },
  pager: { display: "flex", alignItems: "center", gap: 8, marginTop: 8, flexWrap: "wrap" as const },
  pagerInfo: { flex: 1, fontSize: 11, color: "var(--text-muted)", fontFamily: "var(--font-mono)" },
};

export default function CategoriesPanel() {
  const { t } = useTranslation();
  // Automation ▸ Station settings opens the settings box and brings it into view.
  const settingsBox = useRef<HTMLDetailsElement>(null);
  useCommand("categories.settings", useCallback(() => {
    if (settingsBox.current) {
      settingsBox.current.open = true;
      settingsBox.current.scrollIntoView({ block: "start" });
    }
  }, []));
  const [categories, setCategories] = useState<Category[]>([]);
  const [rules, setRules] = useState<CategoryRule[]>([]);
  const [clocks, setClocks] = useState<Clock[]>([]);
  const [config, setConfig] = useState<AutomationConfig | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [draft, setDraft] = useState<Category | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [memberWeight, setMemberWeight] = useState(1);
  const [members, setMembers] = useState<CategoryTrack[]>([]);
  const [memberTotal, setMemberTotal] = useState(0);
  const [memberOffset, setMemberOffset] = useState(0);
  const [selectedMembers, setSelectedMembers] = useState<Set<number>>(() => new Set());
  const [ruleField, setRuleField] = useState<RuleField>("Genre");
  const [rulePattern, setRulePattern] = useState("");

  const reloadCategories = useCallback(async () => {
    const list = await automation.categories();
    setCategories(list);
    return list;
  }, []);

  const loadMembers = useCallback(async (categoryId: number, offset: number) => {
    try {
      let page = await automation.categoryMembers(categoryId, MEMBERS_PAGE, offset);
      // The last page emptied by a removal: step back to the last one with tracks.
      if (page.tracks.length === 0 && page.total > 0 && offset > 0) {
        offset = Math.floor((page.total - 1) / MEMBERS_PAGE) * MEMBERS_PAGE;
        page = await automation.categoryMembers(categoryId, MEMBERS_PAGE, offset);
        setMemberOffset(offset);
      }
      setMembers(page.tracks);
      setMemberTotal(page.total);
      setSelectedMembers(new Set());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    if (!draft || draft.id === 0) {
      setMembers([]);
      setMemberTotal(0);
      setMemberOffset(0);
      setSelectedMembers(new Set());
      return;
    }
    setMemberOffset(0);
    void loadMembers(draft.id, 0);
  }, [draft?.id, loadMembers]);

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const [cats, rls, clks, cfg] = await Promise.all([
          automation.categories(), automation.rules(), automation.clocks(), automation.getConfig(),
        ]);
        if (!alive) return;
        setCategories(cats);
        setRules(rls);
        setClocks(clks);
        setConfig(cfg);
        if (cats.length > 0) {
          setSelectedId(cats[0].id);
          setDraft(clone(cats[0]));
        }
      } catch (e) {
        if (alive) setError(String(e));
      }
    })();
    return () => { alive = false; };
  }, []);

  const selectCategory = (cat: Category) => {
    setSelectedId(cat.id);
    setDraft(clone(cat));
    setError(null);
    setNotice(null);
  };

  const newCategory = () => {
    setSelectedId(null);
    setDraft(blankCategory());
    setError(null);
    setNotice(null);
  };

  const patch = (p: Partial<Category>) => setDraft((d) => (d ? { ...d, ...p } : d));
  const patchFilter = (p: Partial<TrackFilter>) =>
    setDraft((d) => (d ? { ...d, filter: { ...(d.filter ?? {}), ...p } } : d));
  const patchSeparation = (p: Partial<Separation>) =>
    setDraft((d) => (d && d.separation ? { ...d, separation: { ...d.separation, ...p } } : d));

  const saveCategory = async () => {
    if (!draft) return;
    try {
      const saved = await automation.saveCategory(draft);
      await reloadCategories();
      setSelectedId(saved.id);
      setDraft(clone(saved));
      await loadMembers(saved.id, memberOffset);
      setNotice(t("automation.categories.saved"));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const deleteCategory = async () => {
    if (selectedId == null) return;
    const cat = categories.find((c) => c.id === selectedId);
    if (!window.confirm(t("automation.categories.confirmDelete", { name: cat?.name ?? "" }))) return;
    try {
      await automation.deleteCategory(selectedId);
      await reloadCategories();
      setRules(await automation.rules());
      setSelectedId(null);
      setDraft(null);
      setNotice(null);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const addTracks = async (tracks: PickedTrack[]) => {
    if (!draft || draft.id === 0) {
      setError(t("automation.categories.saveFirst"));
      return;
    }
    try {
      const weight = Math.min(100, Math.max(0.01, memberWeight));
      const count = await automation.categoryTracks(draft.id, tracks.map((tr) => tr.id), weight);
      await loadMembers(draft.id, memberOffset);
      setNotice(t("automation.categories.addedTracks", { count }));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const toggleMember = (trackId: number) => setSelectedMembers((prev) => {
    const next = new Set(prev);
    if (next.has(trackId)) next.delete(trackId);
    else next.add(trackId);
    return next;
  });

  const removeSelectedMembers = async () => {
    if (!draft || draft.id === 0) return;
    const ids = [...selectedMembers];
    if (ids.length === 0) return;
    if (!window.confirm(t("automation.categories.confirmRemoveTracks", { count: ids.length }))) return;
    try {
      const count = await automation.categoryTracks(draft.id, ids, 1, true);
      await loadMembers(draft.id, memberOffset);
      setNotice(t("automation.categories.removedTracks", { count }));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const memberPage = (offset: number) => {
    if (!draft || draft.id === 0) return;
    setMemberOffset(offset);
    void loadMembers(draft.id, offset);
  };

  const addRule = async () => {
    if (!draft || draft.id === 0) {
      setError(t("automation.categories.saveFirst"));
      return;
    }
    if (rulePattern.trim() === "") return;
    try {
      await automation.saveRule({ id: 0, category_id: draft.id, field: ruleField, pattern: rulePattern });
      setRules(await automation.rules());
      setRulePattern("");
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const deleteRule = async (rule: CategoryRule) => {
    if (!window.confirm(t("automation.categories.confirmDeleteRule", { pattern: rule.pattern }))) return;
    try {
      await automation.deleteRule(rule.id);
      setRules(await automation.rules());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const applyRules = async () => {
    try {
      const count = await automation.applyRules();
      setNotice(t("automation.categories.rulesApplied", { count }));
      await reloadCategories();
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const patchConfig = (p: Partial<AutomationConfig>) =>
    setConfig((c) => (c ? { ...c, ...p } : c));
  const patchConfigSeparation = (p: Partial<Separation>) =>
    setConfig((c) => (c ? { ...c, separation: { ...c.separation, ...p } } : c));
  const patchImaging = (p: Partial<ImagingRules>) =>
    setConfig((c) => (c ? { ...c, imaging: { ...c.imaging, ...p } } : c));
  const toggleNoOverlap = (kind: ItemKind) =>
    setConfig((c) => (c
      ? {
          ...c,
          no_overlap_kinds: c.no_overlap_kinds.includes(kind)
            ? c.no_overlap_kinds.filter((k) => k !== kind)
            : [...c.no_overlap_kinds, kind],
        }
      : c));

  const saveSettings = async () => {
    if (!config) return;
    try {
      const saved = await automation.saveConfig(config);
      setConfig(saved);
      setNotice(t("automation.categories.settingsSaved"));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const tree = buildTree(categories, null);
  const flat = flattenTree(tree);
  const excludedParents = draft && draft.id > 0
    ? new Set<number>([draft.id, ...descendants(draft.id, categories)])
    : new Set<number>();
  const categoryRules = draft && draft.id > 0 ? rules.filter((r) => r.category_id === draft.id) : [];

  const renderTree = (nodes: TreeNode[], depth: number): React.ReactNode =>
    nodes.map(({ cat, children }) => (
      <div key={cat.id}>
        <div
          style={{
            ...S.row,
            paddingLeft: 10 + depth * 14,
            ...(selectedId === cat.id ? S.rowActive : {}),
          }}
          onClick={() => selectCategory(cat)}
        >
          <span style={{ ...S.swatch, background: cat.color }} />
          <span style={S.rowName}>{cat.name || t("automation.categories.unnamed")}</span>
          <KindBadge kind={cat.kind} />
        </div>
        {renderTree(children, depth + 1)}
      </div>
    ));

  return (
    <div style={S.panel}>
      <div style={S.toolbar}>
        <h2 style={S.title}>{t("automation.categories.title")}</h2>
        <button style={S.btn} onClick={newCategory}>{t("automation.categories.new")}</button>
        <button style={S.btn} onClick={deleteCategory} disabled={selectedId == null}>
          {t("automation.categories.delete")}
        </button>
      </div>

      {(error || notice) && (
        <div style={{ ...S.noticeBar, color: error ? "var(--red)" : "var(--green)" }}>
          {error ?? notice}
        </div>
      )}

      <div style={S.body}>
        <div style={S.left}>
          <div style={S.list}>
            {categories.length === 0 && <div style={S.empty}>{t("automation.categories.empty")}</div>}
            {renderTree(tree, 0)}
          </div>
        </div>

        <div style={S.editor}>
          {!draft && <div style={S.empty}>{t("automation.categories.noSelection")}</div>}

          {draft && (
            <>
              <div style={S.section}>
                <div style={S.sectionTitle}>{t("automation.categories.editor")}</div>
                <div style={S.grid}>
                  <label style={S.field}>{t("automation.categories.name")}
                    <input style={S.input} value={draft.name} onChange={(e) => patch({ name: e.target.value })} />
                  </label>
                  <label style={S.field}>{t("automation.categories.kind")}
                    <select
                      style={S.input}
                      value={draft.kind}
                      onChange={(e) => patch({ kind: e.target.value as ItemKind })}
                    >
                      {ITEM_KINDS.map((k) => (
                        <option key={k} value={k}>{t(`automation.kinds.${k}`, k)}</option>
                      ))}
                    </select>
                  </label>
                  <label style={S.field}>{t("automation.categories.color")}
                    <span style={{ display: "flex", gap: 4 }}>
                      <input
                        type="color"
                        aria-label={t("automation.categories.color")}
                        style={{ width: 34, height: 30, padding: 2 }}
                        value={hexColor(draft.color)}
                        onChange={(e) => patch({ color: e.target.value })}
                      />
                      <input
                        style={{ ...S.input, flex: 1, minWidth: 0 }}
                        value={draft.color}
                        onChange={(e) => patch({ color: e.target.value })}
                      />
                    </span>
                  </label>
                  <label style={S.field}>{t("automation.categories.parent")}
                    <select
                      style={S.input}
                      value={draft.parent_id ?? 0}
                      onChange={(e) => patch({ parent_id: Number(e.target.value) || null })}
                    >
                      <option value={0}>{t("automation.categories.none")}</option>
                      {flat.filter(({ cat }) => !excludedParents.has(cat.id)).map(({ cat, depth }) => (
                        <option key={cat.id} value={cat.id}>
                          {"\u00a0".repeat(depth * 2)}{cat.name || t("automation.categories.unnamed")}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>

                <label style={S.check}>
                  <input
                    type="checkbox"
                    checked={draft.filter !== null}
                    onChange={(e) => patch({ filter: e.target.checked ? blankFilter() : null })}
                  />
                  {t("automation.categories.smart")}
                </label>

                {draft.filter && (
                  <div style={S.grid}>
                    <label style={S.field}>{t("automation.categories.genre")}
                      <input style={S.input} value={draft.filter.genre ?? ""}
                        onChange={(e) => patchFilter({ genre: e.target.value || null })} />
                    </label>
                    <label style={S.field}>{t("automation.categories.artist")}
                      <input style={S.input} value={draft.filter.artist ?? ""}
                        onChange={(e) => patchFilter({ artist: e.target.value || null })} />
                    </label>
                    <label style={S.field}>{t("automation.categories.folder")}
                      <input style={S.input} value={draft.filter.folder ?? ""}
                        onChange={(e) => patchFilter({ folder: e.target.value || null })} />
                    </label>
                    <label style={S.field}>{t("automation.categories.yearFrom")}
                      <input style={S.input} type="number" value={draft.filter.year_from ?? ""}
                        onChange={(e) => patchFilter({ year_from: optNum(e.target.value) })} />
                    </label>
                    <label style={S.field}>{t("automation.categories.yearTo")}
                      <input style={S.input} type="number" value={draft.filter.year_to ?? ""}
                        onChange={(e) => patchFilter({ year_to: optNum(e.target.value) })} />
                    </label>
                    <label style={S.field}>{t("automation.categories.energyMin")}
                      <input style={S.input} type="number" min={0} max={5} value={draft.filter.energy_min ?? ""}
                        onChange={(e) => patchFilter({ energy_min: optNum(e.target.value) })} />
                    </label>
                    <label style={S.field}>{t("automation.categories.energyMax")}
                      <input style={S.input} type="number" min={0} max={5} value={draft.filter.energy_max ?? ""}
                        onChange={(e) => patchFilter({ energy_max: optNum(e.target.value) })} />
                    </label>
                    <label style={S.field}>{t("automation.categories.minDuration")} ({t("automation.categories.seconds")})
                      <input style={S.input} type="number" min={0} step={0.1}
                        value={draft.filter.min_duration_ms == null ? "" : draft.filter.min_duration_ms / 1000}
                        onChange={(e) => patchFilter({ min_duration_ms: secToMs(e.target.value) })} />
                    </label>
                    <label style={S.field}>{t("automation.categories.maxDuration")} ({t("automation.categories.seconds")})
                      <input style={S.input} type="number" min={0} step={0.1}
                        value={draft.filter.max_duration_ms == null ? "" : draft.filter.max_duration_ms / 1000}
                        onChange={(e) => patchFilter({ max_duration_ms: secToMs(e.target.value) })} />
                    </label>
                  </div>
                )}

                <label style={S.check}>
                  <input
                    type="checkbox"
                    checked={draft.separation !== null}
                    onChange={(e) => patch({ separation: e.target.checked ? blankSeparation() : null })}
                  />
                  {t("automation.categories.ownSeparation")}
                </label>

                {draft.separation && (
                  <div style={S.grid}>
                    <label style={S.field}>{t("automation.categories.songMinutes")}
                      <input style={S.input} type="number" min={0} value={draft.separation.song_minutes}
                        onChange={(e) => patchSeparation({ song_minutes: num(e.target.value) })} />
                    </label>
                    <label style={S.field}>{t("automation.categories.artistMinutes")}
                      <input style={S.input} type="number" min={0} value={draft.separation.artist_minutes}
                        onChange={(e) => patchSeparation({ artist_minutes: num(e.target.value) })} />
                    </label>
                    <label style={S.field}>{t("automation.categories.albumMinutes")}
                      <input style={S.input} type="number" min={0} value={draft.separation.album_minutes}
                        onChange={(e) => patchSeparation({ album_minutes: num(e.target.value) })} />
                    </label>
                    <label style={S.field}>{t("automation.categories.imagingOthers")}
                      <input style={S.input} type="number" min={0} value={draft.separation.imaging_others}
                        onChange={(e) => patchSeparation({ imaging_others: num(e.target.value) })} />
                    </label>
                    <label style={{ ...S.check, margin: "16px 0 0" }}>
                      <input type="checkbox" checked={draft.separation.horizontal}
                        onChange={(e) => patchSeparation({ horizontal: e.target.checked })} />
                      {t("automation.categories.horizontal")}
                    </label>
                  </div>
                )}

                <div style={S.actions}>
                  <button style={S.btn} onClick={saveCategory}>{t("automation.categories.save")}</button>
                </div>
              </div>

              <div style={S.section}>
                <div style={S.sectionTitle}>{t("automation.categories.members")}</div>
                {draft.id === 0 ? (
                  <div style={S.hint}>{t("automation.categories.saveFirst")}</div>
                ) : (
                  <>
                    {draft.filter !== null && (
                      <div style={S.hint}>{t("automation.categories.smartMembersNote")}</div>
                    )}

                    {draft.filter === null && (
                      <>
                        <div style={{ marginTop: 8 }}>
                          <label style={{ ...S.field, width: 110 }}>
                            {t("automation.categories.weight")}
                            <input
                              style={S.input}
                              type="number"
                              min={0.01}
                              max={100}
                              step={0.01}
                              value={memberWeight}
                              onChange={(e) => setMemberWeight(num(e.target.value, 1))}
                            />
                          </label>
                        </div>
                        <div style={{ marginTop: 8 }}>
                          <TrackPicker multi onPick={addTracks} />
                        </div>
                      </>
                    )}

                    <div style={S.tableWrap}>
                      <table style={S.table}>
                        <thead>
                          <tr>
                            {draft.filter === null && <th style={{ ...S.th, ...S.thCheck }} />}
                            <th style={S.th}>{t("automation.categories.trackTitle")}</th>
                            <th style={S.th}>{t("automation.categories.artist")}</th>
                            <th style={S.th}>{t("automation.categories.length")}</th>
                            <th style={{ ...S.th, textAlign: "right" as const }}>
                              {t("automation.categories.weightColumn")}
                            </th>
                            <th style={S.th}>{t("automation.categories.lastPlayed")}</th>
                          </tr>
                        </thead>
                        <tbody>
                          {members.length === 0 && (
                            <tr>
                              <td
                                colSpan={draft.filter === null ? 6 : 5}
                                style={{ ...S.td, textAlign: "center", color: "var(--text-muted)" }}
                              >
                                {t("automation.categories.noMembers")}
                              </td>
                            </tr>
                          )}
                          {members.map((track) => (
                            <tr key={track.track_id}>
                              {draft.filter === null && (
                                <td style={S.td}>
                                  <input
                                    type="checkbox"
                                    checked={selectedMembers.has(track.track_id)}
                                    aria-label={track.title}
                                    onChange={() => toggleMember(track.track_id)}
                                  />
                                </td>
                              )}
                              <td style={S.td}>{track.title}</td>
                              <td style={S.td}>{track.artist}</td>
                              <td style={S.tdMono}>{fmtDuration(track.duration_ms)}</td>
                              <td style={{ ...S.tdMono, textAlign: "right" as const }}>{track.weight}</td>
                              <td style={S.tdMono}>{lastPlayed(track.last_played_at)}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>

                    <div style={S.pager}>
                      <button
                        style={S.btn}
                        disabled={memberOffset === 0}
                        onClick={() => memberPage(Math.max(0, memberOffset - MEMBERS_PAGE))}
                      >
                        {t("automation.categories.previous")}
                      </button>
                      <button
                        style={S.btn}
                        disabled={memberOffset + MEMBERS_PAGE >= memberTotal}
                        onClick={() => memberPage(memberOffset + MEMBERS_PAGE)}
                      >
                        {t("automation.categories.next")}
                      </button>
                      <span style={S.pagerInfo}>
                        {t("automation.categories.membersRange", {
                          from: memberTotal === 0 ? 0 : memberOffset + 1,
                          to: Math.min(memberOffset + MEMBERS_PAGE, memberTotal),
                          total: memberTotal,
                        })}
                      </span>
                      {draft.filter === null && (
                        <button
                          style={S.btn}
                          onClick={removeSelectedMembers}
                          disabled={selectedMembers.size === 0}
                        >
                          {t("automation.categories.removeSelected")}
                        </button>
                      )}
                    </div>
                  </>
                )}
              </div>

              <div style={S.section}>
                <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
                  <div style={{ ...S.sectionTitle, marginBottom: 0, flex: 1 }}>
                    {t("automation.categories.rules")}
                  </div>
                  <button style={S.btn} onClick={applyRules} disabled={draft.id === 0}>
                    {t("automation.categories.applyRules")}
                  </button>
                </div>
                {draft.id === 0 ? (
                  <div style={S.hint}>{t("automation.categories.saveFirst")}</div>
                ) : (
                  <>
                    {categoryRules.length === 0 && (
                      <div style={S.hint}>{t("automation.categories.noRules")}</div>
                    )}
                    {categoryRules.map((rule) => (
                      <div key={rule.id} style={S.line}>
                        <span style={S.lineMain}>
                          {t(`automation.categories.fields.${rule.field}`, rule.field)}: {rule.pattern}
                        </span>
                        <button
                          style={S.btn}
                          aria-label={t("automation.categories.deleteRule")}
                          onClick={() => deleteRule(rule)}
                        >
                          &#10005;
                        </button>
                      </div>
                    ))}
                    <div style={S.actions}>
                      <label style={S.field}>{t("automation.categories.ruleField")}
                        <select style={S.input} value={ruleField}
                          onChange={(e) => setRuleField(e.target.value as RuleField)}>
                          <option value="Genre">{t("automation.categories.fields.Genre")}</option>
                          <option value="Artist">{t("automation.categories.fields.Artist")}</option>
                          <option value="Folder">{t("automation.categories.fields.Folder")}</option>
                        </select>
                      </label>
                      <label style={{ ...S.field, flex: 1 }}>{t("automation.categories.rulePattern")}
                        <input style={S.input} value={rulePattern}
                          onChange={(e) => setRulePattern(e.target.value)} />
                      </label>
                      <button style={S.btn} onClick={addRule} disabled={rulePattern.trim() === ""}>
                        {t("automation.categories.addRule")}
                      </button>
                    </div>
                  </>
                )}
              </div>
            </>
          )}
        </div>
      </div>

      <details ref={settingsBox} style={S.settings}>
        <summary style={S.summary}>{t("automation.categories.settings")}</summary>
        <div style={S.settingsBody}>
          {!config && <div style={S.hint}>{t("automation.categories.settingsUnavailable")}</div>}
          {config && (
            <>
              <label style={S.check}>
                <input type="checkbox" checked={config.enabled}
                  onChange={(e) => patchConfig({ enabled: e.target.checked })} />
                {t("automation.categories.enabled")}
              </label>

              <div style={S.grid}>
                <label style={S.field}>{t("automation.categories.defaultClock")}
                  <select style={S.input} value={config.default_clock ?? ""}
                    onChange={(e) => patchConfig({ default_clock: e.target.value || null })}>
                    <option value="">{t("automation.categories.none")}</option>
                    {clocks.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
                  </select>
                </label>
                <label style={S.field}>{t("automation.categories.fallbackCategory")}
                  <select style={S.input} value={config.fallback_category ?? 0}
                    onChange={(e) => patchConfig({ fallback_category: Number(e.target.value) || null })}>
                    <option value={0}>{t("automation.categories.none")}</option>
                    {categories.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
                  </select>
                </label>
              </div>

              <div style={S.sectionTitle}>{t("automation.categories.defaultSeparation")}</div>
              <div style={S.grid}>
                <label style={S.field}>{t("automation.categories.songMinutes")}
                  <input style={S.input} type="number" min={0} value={config.separation.song_minutes}
                    onChange={(e) => patchConfigSeparation({ song_minutes: num(e.target.value) })} />
                </label>
                <label style={S.field}>{t("automation.categories.artistMinutes")}
                  <input style={S.input} type="number" min={0} value={config.separation.artist_minutes}
                    onChange={(e) => patchConfigSeparation({ artist_minutes: num(e.target.value) })} />
                </label>
                <label style={S.field}>{t("automation.categories.albumMinutes")}
                  <input style={S.input} type="number" min={0} value={config.separation.album_minutes}
                    onChange={(e) => patchConfigSeparation({ album_minutes: num(e.target.value) })} />
                </label>
                <label style={S.field}>{t("automation.categories.imagingOthers")}
                  <input style={S.input} type="number" min={0} value={config.separation.imaging_others}
                    onChange={(e) => patchConfigSeparation({ imaging_others: num(e.target.value) })} />
                </label>
                <label style={{ ...S.check, margin: "16px 0 0" }}>
                  <input type="checkbox" checked={config.separation.horizontal}
                    onChange={(e) => patchConfigSeparation({ horizontal: e.target.checked })} />
                  {t("automation.categories.horizontal")}
                </label>
              </div>

              <div style={S.sectionTitle}>{t("automation.categories.imaging")}</div>
              <div style={S.grid}>
                <label style={S.field}>{t("automation.categories.jingleCategory")}
                  <select style={S.input} value={config.imaging.jingle_category ?? 0}
                    onChange={(e) => patchImaging({ jingle_category: Number(e.target.value) || null })}>
                    <option value={0}>{t("automation.categories.none")}</option>
                    {categories.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
                  </select>
                </label>
                <label style={S.field}>{t("automation.categories.jingleEvery")}
                  <input style={S.input} type="number" min={0} value={config.imaging.jingle_every}
                    onChange={(e) => patchImaging({ jingle_every: num(e.target.value) })} />
                </label>
                <label style={S.field}>{t("automation.categories.sweeperOnChange")}
                  <select style={S.input} value={config.imaging.sweeper_on_change ?? 0}
                    onChange={(e) => patchImaging({ sweeper_on_change: Number(e.target.value) || null })}>
                    <option value={0}>{t("automation.categories.none")}</option>
                    {categories.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
                  </select>
                </label>
              </div>

              <div style={S.grid}>
                <label style={S.field}>{t("automation.categories.minItemsAhead")}
                  <input style={S.input} type="number" min={1} max={50} value={config.min_items_ahead}
                    onChange={(e) => patchConfig({ min_items_ahead: num(e.target.value) })} />
                </label>
                <label style={S.field}>{t("automation.categories.minMinutesAhead")}
                  <input style={S.input} type="number" min={1} max={240} value={config.min_minutes_ahead}
                    onChange={(e) => patchConfig({ min_minutes_ahead: num(e.target.value) })} />
                </label>
                <label style={S.field}>{t("automation.categories.keepPlayed")}
                  <input style={S.input} type="number" min={0} max={500} value={config.keep_played}
                    onChange={(e) => patchConfig({ keep_played: num(e.target.value) })} />
                </label>
                <label style={S.field}>{t("automation.categories.adBreakSeconds")}
                  <input style={S.input} type="number" min={10} max={1800} value={config.ad_break_seconds}
                    onChange={(e) => patchConfig({ ad_break_seconds: num(e.target.value) })} />
                </label>
              </div>

              <div style={S.sectionTitle}>{t("automation.categories.noOverlapKinds")}</div>
              <div style={S.chips}>
                {ITEM_KINDS.map((kind) => (
                  <label key={kind} style={{ ...S.check, margin: 0 }}>
                    <input type="checkbox" checked={config.no_overlap_kinds.includes(kind)}
                      onChange={() => toggleNoOverlap(kind)} />
                    {t(`automation.kinds.${kind}`, kind)}
                  </label>
                ))}
              </div>

              <div style={S.actions}>
                <button style={S.btn} onClick={saveSettings}>
                  {t("automation.categories.saveSettings")}
                </button>
              </div>
            </>
          )}
        </div>
      </details>
    </div>
  );
}
