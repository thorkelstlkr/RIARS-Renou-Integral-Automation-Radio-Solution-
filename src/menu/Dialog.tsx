import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";

const S: Record<string, React.CSSProperties> = {
  backdrop: {
    position: "fixed", inset: 0, background: "rgba(0,0,0,0.55)", zIndex: 900,
    display: "flex", alignItems: "center", justifyContent: "center",
  },
  box: {
    background: "var(--bg-secondary)", border: "1px solid var(--border)", borderRadius: 8,
    boxShadow: "0 12px 40px rgba(0,0,0,0.5)", maxWidth: "calc(100vw - 48px)", maxHeight: "calc(100vh - 48px)",
    display: "flex", flexDirection: "column",
  },
  head: {
    display: "flex", alignItems: "center", justifyContent: "space-between", padding: "10px 16px",
    borderBottom: "1px solid var(--border)", fontWeight: 600,
  },
  close: { fontSize: 16, padding: "2px 8px", color: "var(--text-secondary)" },
  body: { padding: 16, overflowY: "auto" },
};

/** A modal dialog: Esc, the close button or a click outside closes it. */
export default function Dialog({ title, width = 460, onClose, children }: {
  title: string; width?: number; onClose: () => void; children: React.ReactNode;
}) {
  const { t } = useTranslation();
  const box = useRef<HTMLDivElement>(null);
  // Callers often pass a new onClose on every render (an inline arrow under a panel
  // that refreshes several times a second). Read it through a ref, so the effects
  // below run once: re-running them moved the focus back to the first field, which
  // closed an open dropdown and took the cursor out of the field being typed in.
  const close = useRef(onClose);
  close.current = onClose;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") { e.stopPropagation(); close.current(); } };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  // Focus the first field once, when the dialog opens.
  useEffect(() => {
    box.current?.querySelector<HTMLElement>("input,select,button")?.focus();
  }, []);

  return (
    <div style={S.backdrop} onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div ref={box} role="dialog" aria-modal="true" aria-label={title} style={{ ...S.box, width }}>
        <div style={S.head}>
          <span>{title}</span>
          <button style={S.close} onClick={onClose} aria-label={t("menu.close")}>✕</button>
        </div>
        <div style={S.body}>{children}</div>
      </div>
    </div>
  );
}
