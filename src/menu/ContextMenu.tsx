import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { MenuEntry, MenuItem } from "./MenuBar";

const S: Record<string, React.CSSProperties> = {
  overlay: { position: "fixed", inset: 0, zIndex: 1000 },
  menu: {
    position: "fixed", minWidth: 230, maxWidth: 360, padding: "4px 0", background: "var(--bg-panel)",
    border: "1px solid var(--border)", borderRadius: 6, boxShadow: "0 8px 24px rgba(0,0,0,0.45)",
    fontSize: 13, userSelect: "none",
  },
  title: {
    padding: "4px 12px 6px", fontSize: 11, color: "var(--text-muted)", borderBottom: "1px solid var(--border)",
    marginBottom: 4, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
  },
  item: {
    display: "flex", alignItems: "center", width: "100%", padding: "5px 12px", gap: 12,
    textAlign: "left", fontSize: 13, color: "var(--text-primary)", background: "transparent",
  },
  active: { background: "var(--accent)", color: "#fff" },
  danger: { color: "var(--red)" },
  check: { width: 16, textAlign: "center" as const, fontSize: 12, flexShrink: 0 },
  label: { flex: 1, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" },
  shortcut: { fontSize: 11, opacity: 0.7, fontFamily: "var(--font-mono)", whiteSpace: "nowrap" },
  sep: { height: 1, background: "var(--border)", margin: "4px 0" },
};

export interface ContextItem extends MenuItem {
  /** Shown in red: removes or deletes something. */
  danger?: boolean;
}
export type ContextEntry = ContextItem | "separator";

const isItem = (e: ContextEntry | MenuEntry): e is ContextItem => e !== "separator";

/**
 * Right-click menu at (x, y), kept inside the window. Arrow keys, Home/End, Enter and
 * Escape work; a click outside, a scroll or a resize closes it. Items run their action
 * after the menu has closed.
 */
export default function ContextMenu({ x, y, title, items, onClose }: {
  x: number; y: number; title?: string; items: ContextEntry[]; onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });
  const enabled = items.map((e, i) => (isItem(e) && !e.disabled ? i : -1)).filter((i) => i >= 0);
  const [active, setActive] = useState(-1);

  // Open towards the inside when the click is near the right or bottom edge.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    setPos({
      left: Math.max(4, Math.min(x, window.innerWidth - width - 4)),
      top: Math.max(4, Math.min(y, window.innerHeight - height - 4)),
    });
    el.focus();
  }, [x, y]);

  const run = (item: ContextItem) => {
    if (item.disabled) return;
    onClose();
    // After the close, so a dialog opened by the action gets the focus.
    setTimeout(() => item.action?.(), 0);
  };

  useEffect(() => {
    const close = () => onClose();
    window.addEventListener("resize", close);
    window.addEventListener("blur", close);
    window.addEventListener("scroll", close, true);
    return () => {
      window.removeEventListener("resize", close);
      window.removeEventListener("blur", close);
      window.removeEventListener("scroll", close, true);
    };
  }, [onClose]);

  const onKey = (e: React.KeyboardEvent) => {
    e.stopPropagation();
    const at = enabled.indexOf(active);
    // With nothing highlighted, Down starts at the first item and Up at the last.
    const step = (n: number) => setActive(
      at < 0 ? enabled[n > 0 ? 0 : enabled.length - 1] ?? -1 : enabled[(at + n + enabled.length) % enabled.length],
    );
    switch (e.key) {
      case "ArrowDown": e.preventDefault(); step(1); break;
      case "ArrowUp": e.preventDefault(); step(-1); break;
      case "Home": e.preventDefault(); setActive(enabled[0] ?? -1); break;
      case "End": e.preventDefault(); setActive(enabled[enabled.length - 1] ?? -1); break;
      case "Enter": case " ": {
        e.preventDefault();
        const item = items[active];
        if (item && isItem(item)) run(item);
        break;
      }
      case "Escape": case "Tab": e.preventDefault(); onClose(); break;
    }
  };

  return (
    <div style={S.overlay} onMouseDown={onClose} onContextMenu={(e) => { e.preventDefault(); onClose(); }}>
      <div ref={ref} role="menu" aria-label={title} tabIndex={-1} style={{ ...S.menu, ...pos, outline: "none" }}
        onMouseDown={(e) => e.stopPropagation()} onKeyDown={onKey}>
        {title && <div style={S.title} title={title}>{title}</div>}
        {items.map((entry, i) => entry === "separator" ? (
          <div key={i} role="separator" style={S.sep} />
        ) : (
          <button key={i} role={entry.checked !== undefined ? "menuitemcheckbox" : "menuitem"}
            aria-checked={entry.checked} disabled={entry.disabled} tabIndex={-1}
            style={{
              ...S.item, ...(entry.danger ? S.danger : {}), ...(i === active ? S.active : {}),
              opacity: entry.disabled ? 0.45 : 1,
            }}
            onMouseEnter={() => setActive(entry.disabled ? -1 : i)} onClick={() => run(entry)}>
            <span style={S.check}>{entry.checked ? "✓" : ""}</span>
            <span style={S.label}>{entry.label}</span>
            {entry.shortcut && <span style={S.shortcut}>{entry.shortcut}</span>}
          </button>
        ))}
      </div>
    </div>
  );
}
