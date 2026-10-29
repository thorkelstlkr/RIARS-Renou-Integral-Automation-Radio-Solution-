import { useState } from "react";
import { useTranslation } from "react-i18next";
import ContextMenu, { type ContextEntry } from "../menu/ContextMenu";
import { tabsApi, type PlaylistTab, type Tabs } from "./playlistTabs";

const S: Record<string, React.CSSProperties> = {
  bar: {
    display: "flex", alignItems: "flex-end", gap: 2, padding: "6px 12px 0", overflowX: "auto",
    borderBottom: "1px solid var(--border)", background: "var(--bg-secondary)",
  },
  tab: {
    display: "flex", alignItems: "center", gap: 6, padding: "5px 10px", fontSize: 12, maxWidth: 220,
    borderRadius: "var(--radius) var(--radius) 0 0", background: "transparent", color: "var(--text-secondary)",
    border: "1px solid transparent", borderBottom: "none", cursor: "pointer", whiteSpace: "nowrap" as const,
  },
  shown: { background: "var(--bg-primary)", color: "var(--text-primary)", borderColor: "var(--border)" },
  name: { overflow: "hidden", textOverflow: "ellipsis" },
  dot: { width: 8, height: 8, borderRadius: "50%", background: "var(--red)", flexShrink: 0 },
  count: { color: "var(--text-muted)", fontFamily: "var(--font-mono)", fontSize: 11 },
  close: { padding: "0 3px", fontSize: 11, background: "transparent", color: "var(--text-muted)" },
  add: { padding: "4px 10px", fontSize: 14, background: "transparent", color: "var(--text-secondary)" },
  rename: { fontSize: 12, width: 140 },
};

/** The tab on air before tabs existed is called "On air" by the engine. */
export const tabName = (tab: { name: string }, t: (k: string) => string) =>
  tab.name === "On air" ? t("playlist.tabs.main") : tab.name;

/**
 * Tabs above the playlist: one per list open for editing. The one on air carries a red
 * dot; `shown` is the one in the panel. Renames on double click, closes with ✕ (asking
 * first when the list has items), and has a right-click menu.
 */
export default function PlaylistTabBar({ tabs, shown, onShow, onTabs, onError }: {
  tabs: Tabs;
  shown: string;
  onShow: (id: string) => void;
  onTabs: (tabs: Tabs) => void;
  onError: (error: string | null) => void;
}) {
  const { t } = useTranslation();
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; tab: PlaylistTab } | null>(null);

  const act = async (f: () => Promise<Tabs>) => {
    try {
      onTabs(await f());
      onError(null);
    } catch (e) {
      onError(String(e));
    }
  };

  const add = async () => {
    // "List 2", "List 3"...: the first name not taken.
    const taken = new Set(tabs.tabs.map((tab) => tab.name.toLowerCase()));
    let n = tabs.tabs.length + 1;
    while (taken.has(t("playlist.tabs.newName", { n }).toLowerCase())) n++;
    try {
      const list = await tabsApi.open(t("playlist.tabs.newName", { n }));
      onTabs(await tabsApi.list());
      onShow(list.id);
      onError(null);
    } catch (e) {
      onError(String(e));
    }
  };

  const close = (tab: PlaylistTab) => {
    if (tab.items > 0 && !window.confirm(t("playlist.tabs.confirmClose", { name: tabName(tab, t), count: tab.items }))) return;
    void act(() => tabsApi.close(tab.id));
  };

  const finishRename = () => {
    if (!renaming) return;
    const { id, name } = renaming;
    setRenaming(null);
    if (name.trim()) void act(() => tabsApi.rename(id, name));
  };

  const menuItems = (tab: PlaylistTab): ContextEntry[] => [
    {
      label: t("playlist.tabs.putOnAir"), disabled: tab.id === tabs.on_air,
      action: () => { void act(() => tabsApi.onAir(tab.id)); },
    },
    { label: t("playlist.tabs.rename"), action: () => setRenaming({ id: tab.id, name: tabName(tab, t) }) },
    "separator",
    {
      label: t("playlist.tabs.close"), danger: true, disabled: tab.id === tabs.on_air,
      action: () => close(tab),
    },
  ];

  return (
    <div style={S.bar} role="tablist" data-testid="playlist-tabs">
      {tabs.tabs.map((tab) => {
        const onAir = tab.id === tabs.on_air;
        return (
          <div key={tab.id} role="tab" aria-selected={tab.id === shown} data-testid="playlist-tab"
            style={{ ...S.tab, ...(tab.id === shown ? S.shown : {}) }}
            title={onAir ? t("playlist.tabs.onAirHint") : tabName(tab, t)}
            onClick={() => onShow(tab.id)}
            onDoubleClick={() => setRenaming({ id: tab.id, name: tabName(tab, t) })}
            onContextMenu={(e) => { e.preventDefault(); setMenu({ x: e.clientX, y: e.clientY, tab }); }}>
            {onAir && <span style={S.dot} aria-label={t("playlist.tabs.onAir")} />}
            {renaming?.id === tab.id
              ? <input style={S.rename} value={renaming.name} autoFocus aria-label={t("playlist.tabs.rename")}
                  onChange={(e) => setRenaming({ id: tab.id, name: e.target.value })}
                  onBlur={finishRename}
                  onClick={(e) => e.stopPropagation()}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") finishRename();
                    if (e.key === "Escape") setRenaming(null);
                  }} />
              : <span style={S.name}>{tabName(tab, t)}</span>}
            <span style={S.count}>{tab.items}</span>
            {!onAir && (
              <button style={S.close} title={t("playlist.tabs.close")} aria-label={t("playlist.tabs.close")}
                onClick={(e) => { e.stopPropagation(); close(tab); }}>✕</button>
            )}
          </div>
        );
      })}
      <button style={S.add} title={t("playlist.tabs.add")} aria-label={t("playlist.tabs.add")}
        onClick={() => { void add(); }} data-testid="playlist-tab-add">+</button>
      {menu && (
        <ContextMenu x={menu.x} y={menu.y} title={tabName(menu.tab, t)} items={menuItems(menu.tab)} onClose={() => setMenu(null)} />
      )}
    </div>
  );
}
