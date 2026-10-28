import { useCallback, useEffect, useRef, useState } from "react";

export interface MenuItem {
  label: string;
  shortcut?: string;
  action?: () => void;
  /** Shows a check mark (on/off items, the current tab or language). */
  checked?: boolean;
  disabled?: boolean;
  submenu?: MenuEntry[];
}
export type MenuEntry = MenuItem | "separator";
export interface Menu {
  id: string;
  label: string;
  items: MenuEntry[];
}

const S: Record<string, React.CSSProperties> = {
  bar: {
    display: "flex", alignItems: "stretch", height: 28, padding: "0 6px", gap: 2,
    background: "var(--bg-secondary)", borderBottom: "1px solid var(--border)", fontSize: 13, userSelect: "none",
    position: "relative", zIndex: 800,
  },
  top: { padding: "0 10px", borderRadius: 4, color: "var(--text-primary)" },
  topOpen: { background: "var(--bg-panel)" },
  list: {
    position: "absolute", top: 27, minWidth: 250, padding: "4px 0", background: "var(--bg-panel)",
    border: "1px solid var(--border)", borderRadius: 6, boxShadow: "0 8px 24px rgba(0,0,0,0.45)",
  },
  item: {
    display: "flex", alignItems: "center", width: "100%", padding: "5px 12px 5px 8px", gap: 8,
    textAlign: "left", fontSize: 13, color: "var(--text-primary)",
  },
  itemActive: { background: "var(--accent)", color: "#fff" },
  check: { width: 16, textAlign: "center", fontSize: 12 },
  label: { flex: 1, whiteSpace: "nowrap" },
  shortcut: { fontSize: 11, opacity: 0.7, fontFamily: "var(--font-mono)", whiteSpace: "nowrap" },
  sep: { height: 1, background: "var(--border)", margin: "4px 0" },
};

const isItem = (e: MenuEntry): e is MenuItem => e !== "separator";
const enabledIndexes = (items: MenuEntry[]) =>
  items.map((e, i) => (isItem(e) && !e.disabled ? i : -1)).filter((i) => i >= 0);

function List({ items, active, onActive, onRun, sub, left }: {
  items: MenuEntry[]; active: number; onActive: (i: number) => void; onRun: (item: MenuItem) => void;
  sub: number | null; left: number;
}) {
  return (
    <div role="menu" style={{ ...S.list, left }}>
      {items.map((entry, i) => entry === "separator" ? (
        <div key={i} style={S.sep} role="separator" />
      ) : (
        <div key={i} style={{ position: "relative" }}>
          <button role={entry.checked !== undefined ? "menuitemcheckbox" : "menuitem"}
            aria-checked={entry.checked} aria-haspopup={entry.submenu ? "menu" : undefined}
            disabled={entry.disabled} tabIndex={-1}
            style={{ ...S.item, ...(i === active ? S.itemActive : {}), opacity: entry.disabled ? 0.45 : 1 }}
            onMouseEnter={() => onActive(i)} onClick={() => onRun(entry)}>
            <span style={S.check}>{entry.checked ? "✓" : ""}</span>
            <span style={S.label}>{entry.label}</span>
            {entry.shortcut && <span style={S.shortcut}>{entry.shortcut}</span>}
            {entry.submenu && <span style={S.shortcut}>▸</span>}
          </button>
          {entry.submenu && sub === i && (
            <div style={{ position: "absolute", top: -31, left: 0 }}>
              <List items={entry.submenu} active={-1} onActive={() => {}} onRun={onRun} sub={null} left={246} />
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

/** In-window menu bar, as in RadioBOSS. F10 or Alt focuses it; arrows, Enter and Esc work inside. */
export default function MenuBar({ menus, onOpen }: { menus: Menu[]; onOpen?: (menuId: string) => void }) {
  const [open, setOpen] = useState<number | null>(null);
  const [active, setActive] = useState(-1);
  const [sub, setSub] = useState<number | null>(null);
  const bar = useRef<HTMLDivElement>(null);
  const tops = useRef<(HTMLButtonElement | null)[]>([]);

  const openMenu = useCallback((i: number | null) => {
    setOpen(i);
    setActive(-1);
    setSub(null);
    if (i !== null) onOpen?.(menus[i].id);
  }, [menus, onOpen]);

  const run = useCallback((item: MenuItem) => {
    if (item.disabled) return;
    if (item.submenu) return;
    openMenu(null);
    item.action?.();
  }, [openMenu]);

  // Close on a click outside.
  useEffect(() => {
    if (open === null) return;
    const onDown = (e: MouseEvent) => { if (!bar.current?.contains(e.target as Node)) openMenu(null); };
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [open, openMenu]);

  // Keyboard: F10 / Alt to enter, then arrows, Enter, Esc.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (open === null) {
        if (e.key === "F10" || (e.key === "Alt" && !e.ctrlKey && !e.shiftKey)) {
          e.preventDefault();
          openMenu(0);
          tops.current[0]?.focus();
        }
        return;
      }
      const items = menus[open].items;
      const enabled = enabledIndexes(items);
      const current = active >= 0 ? items[active] : undefined;
      const move = (dir: number) => {
        const pos = enabled.indexOf(active);
        setActive(enabled[(pos + dir + enabled.length) % enabled.length] ?? -1);
        setSub(null);
      };
      switch (e.key) {
        case "Escape": e.preventDefault(); if (sub !== null) setSub(null); else openMenu(null); break;
        case "ArrowDown": e.preventDefault(); move(active < 0 ? 0 : 1); break;
        case "ArrowUp": e.preventDefault(); move(-1); break;
        case "ArrowRight":
          e.preventDefault();
          if (current && isItem(current) && current.submenu) setSub(active);
          else openMenu((open + 1) % menus.length);
          break;
        case "ArrowLeft": e.preventDefault(); if (sub !== null) setSub(null); else openMenu((open - 1 + menus.length) % menus.length); break;
        case "Enter":
          e.preventDefault();
          if (current && isItem(current)) {
            if (current.submenu) setSub(active); else run(current);
          }
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, active, sub, menus, openMenu, run]);

  return (
    <nav ref={bar} role="menubar" style={S.bar}>
      {menus.map((menu, i) => (
        <div key={menu.id} style={{ position: "relative", display: "flex" }}>
          <button ref={(el) => { tops.current[i] = el; }} role="menuitem" aria-haspopup="menu" aria-expanded={open === i}
            style={{ ...S.top, ...(open === i ? S.topOpen : {}) }}
            onMouseDown={(e) => { e.preventDefault(); openMenu(open === i ? null : i); }}
            onMouseEnter={() => { if (open !== null && open !== i) openMenu(i); }}>
            {menu.label}
          </button>
          {open === i && (
            <List items={menu.items} active={active} sub={sub} left={0}
              onActive={(idx) => { setActive(idx); const e = menu.items[idx]; setSub(isItem(e) && e.submenu ? idx : null); }}
              onRun={run} />
          )}
        </div>
      ))}
    </nav>
  );
}
