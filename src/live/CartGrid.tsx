import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { analysis, preview } from "../analysis/api";
import ContextMenu, { type ContextEntry } from "../menu/ContextMenu";
import CartButtonDialog from "./CartButtonDialog";
import {
  CART_COLUMNS, CART_KEYS, CART_PAGES, CART_SLOTS, cartId, live,
  type CartBank, type CartBankSummary, type CartButton, type CartPlaying,
} from "./api";

const S: Record<string, React.CSSProperties> = {
  wrap: { display: "flex", flexDirection: "column" as const, gap: 8 },
  toolbar: { display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" as const },
  title: { fontSize: 12, fontWeight: 700, letterSpacing: 0.6, textTransform: "uppercase" as const, color: "var(--text-secondary)" },
  select: { fontSize: 13, minWidth: 160 },
  btn: { padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12, background: "var(--bg-panel)" },
  danger: { padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12, background: "var(--bg-panel)", color: "var(--red)" },
  stopAll: { padding: "4px 12px", borderRadius: "var(--radius)", fontSize: 12, background: "var(--bg-panel)", color: "var(--red)", marginLeft: "auto" },
  notice: { fontSize: 12, color: "var(--red)", wordBreak: "break-word" as const },
  noBank: {
    display: "flex", alignItems: "center", justifyContent: "center", gap: 10, padding: 16,
    border: "1px dashed var(--border)", borderRadius: "var(--radius)", color: "var(--text-muted)", fontSize: 13,
  },
  pages: { display: "flex", gap: 4, flexWrap: "wrap" as const },
  page: {
    minWidth: 28, padding: "3px 8px", borderRadius: "var(--radius)", fontSize: 12,
    background: "var(--bg-panel)", color: "var(--text-secondary)",
  },
  pageOn: { background: "var(--accent)", color: "#fff" },
  grid: { display: "grid", gap: 6 },
  cell: {
    position: "relative" as const, display: "flex", flexDirection: "column" as const, justifyContent: "space-between",
    minHeight: 72, padding: 6, borderRadius: "var(--radius)", border: "1px solid var(--border)",
    color: "#fff", textAlign: "left" as const, overflow: "hidden", cursor: "pointer",
  },
  cellEmpty: { color: "var(--text-muted)", background: "var(--bg-panel)" },
  cellPlaying: { borderColor: "var(--text-primary)", boxShadow: "0 0 0 1px var(--text-primary) inset" },
  label: {
    fontSize: 12, fontWeight: 600, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const,
    textShadow: "0 1px 2px rgba(0,0,0,0.6)",
  },
  emptyLabel: { fontSize: 11, lineHeight: 1.3 },
  bottom: { display: "flex", alignItems: "center", justifyContent: "space-between", gap: 6, fontSize: 11 },
  mode: { opacity: 0.85 },
  left: { fontFamily: "var(--font-mono)", fontWeight: 600 },
  keyHint: {
    position: "absolute" as const, top: 4, right: 5, fontSize: 10, opacity: 0.65,
    fontFamily: "var(--font-mono)",
  },
  progress: {
    position: "absolute" as const, display: "block", left: 0, right: 0, bottom: 0, height: 3,
    background: "rgba(0,0,0,0.35)",
  },
  fill: { height: "100%", background: "var(--text-primary)", transition: "width 150ms linear" },
};

/** The sound bank: bank selector, 8 pages and the 4 × 6 grid of buttons. */
export default function CartGrid({ carts }: { carts: CartPlaying[] }) {
  const { t } = useTranslation();
  const [banks, setBanks] = useState<CartBankSummary[]>([]);
  const [bank, setBank] = useState<CartBank | null>(null);
  const [activeId, setActiveId] = useState<number | null>(null);
  const [page, setPage] = useState(0);
  const [notice, setNotice] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [ctx, setCtx] = useState<{ x: number; y: number; slot: number } | null>(null);
  const [editing, setEditing] = useState<{ slot: number; button: CartButton | null } | null>(null);
  const [durations, setDurations] = useState<Record<string, number>>({});
  // Time left when each playing button was first seen: the bar's length for files
  // never analysed. Kept in a ref: it changes during render, nothing re-renders for it.
  const firstLeft = useRef<Record<number, number>>({});

  const reloadBank = useCallback(async (id: number | null) => {
    if (id == null) {
      setBank(null);
      return;
    }
    try {
      setBank(await live.bank(id));
    } catch (e) {
      setNotice(String(e));
    }
  }, []);

  const load = useCallback(async () => {
    try {
      const list = await live.banks();
      setBanks(list);
      let id = await live.activeBank();
      if (id == null || !list.some((item) => item.id === id)) {
        id = list[0]?.id ?? null;
        if (id != null) await live.setActiveBank(id).catch(() => {});
      }
      setActiveId(id);
      await reloadBank(id);
      setNotice(null);
    } catch (e) {
      setNotice(String(e));
    } finally {
      setLoading(false);
    }
  }, [reloadBank]);

  useEffect(() => { void load(); }, [load]);

  const selectBank = async (id: number) => {
    try {
      await live.setActiveBank(id);
      setActiveId(id);
      setPage(0);
      await reloadBank(id);
      setNotice(null);
    } catch (e) {
      setNotice(String(e));
    }
  };

  const createBank = async () => {
    try {
      const id = await live.saveBank({ id: 0, name: t("live.defaultBankName"), buttons: [] });
      setBanks(await live.banks());
      await selectBank(id);
    } catch (e) {
      setNotice(String(e));
    }
  };

  const renameBank = async () => {
    if (!bank) return;
    const name = window.prompt(t("live.renameBank"), bank.name);
    if (name == null || name.trim() === "") return;
    try {
      await live.saveBank({ ...bank, name: name.trim() });
      setBanks(await live.banks());
      await reloadBank(bank.id);
      setNotice(null);
    } catch (e) {
      setNotice(String(e));
    }
  };

  const deleteBank = async () => {
    if (!bank) return;
    if (!window.confirm(t("live.confirmDeleteBank"))) return;
    try {
      await live.deleteBank(bank.id);
      await load();
    } catch (e) {
      setNotice(String(e));
    }
  };

  const fire = useCallback(async (button: CartButton) => {
    try {
      await live.fire(button);
      setNotice(null);
    } catch (e) {
      setNotice(String(e));
    }
  }, []);

  const clearButton = async (slot: number) => {
    if (!bank) return;
    try {
      await live.clearButton(bank.id, page, slot);
      await reloadBank(bank.id);
      setNotice(null);
    } catch (e) {
      setNotice(String(e));
    }
  };

  const bySlot = useMemo(() => {
    const map = new Map<number, CartButton>();
    for (const button of bank?.buttons ?? []) {
      map.set(cartId(button.page, button.slot), button);
    }
    return map;
  }, [bank]);

  const playing = useMemo(() => {
    const map = new Map<number, CartPlaying>();
    for (const cart of carts) map.set(cart.button, cart);
    return map;
  }, [carts]);

  const pagePaths = useMemo(() => {
    const paths = (bank?.buttons ?? [])
      .filter((button) => button.page === page && button.file_path)
      .map((button) => button.file_path);
    return Array.from(new Set(paths));
  }, [bank, page]);

  // Keep the page decoded ahead so the first press is instant.
  useEffect(() => {
    if (pagePaths.length === 0) return;
    live.preload(pagePaths).catch(() => { /* the sound plays from disk instead */ });
  }, [pagePaths]);

  // Durations for the progress bars (unknown tracks fall back to an empty bar).
  useEffect(() => {
    if (pagePaths.length === 0) {
      setDurations({});
      return;
    }
    let alive = true;
    analysis.pathsCues(pagePaths)
      .then((list) => {
        if (!alive) return;
        const map: Record<string, number> = {};
        for (const found of list) map[found.path] = found.cues.duration_ms;
        setDurations(map);
      })
      .catch(() => { /* analysis unavailable */ });
    return () => { alive = false; };
  }, [pagePaths]);

  // Keyboard: keys fire the visible page, PageUp/PageDown change page, Escape stops all.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (document.querySelector("[role=dialog]")) return;
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA"
        || target.tagName === "SELECT" || target.isContentEditable)) return;
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key === "PageUp") {
        e.preventDefault();
        setPage((prev) => Math.max(0, prev - 1));
        return;
      }
      if (e.key === "PageDown") {
        e.preventDefault();
        setPage((prev) => Math.min(CART_PAGES - 1, prev + 1));
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        live.stop(null).catch((err) => setNotice(String(err)));
        return;
      }
      const slot = CART_KEYS.indexOf(e.key.toLowerCase());
      if (slot >= 0) {
        const button = bySlot.get(cartId(page, slot));
        if (button) {
          e.preventDefault();
          void fire(button);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [bySlot, page, fire]);

  const dropTrack = async (e: React.DragEvent, slot: number) => {
    e.preventDefault();
    if (!bank) return;
    const raw = e.dataTransfer.getData("application/x-riars-track");
    if (!raw) return;
    let track: { id?: number; title?: string; file_path?: string };
    try {
      track = JSON.parse(raw);
    } catch {
      return;
    }
    if (!track.file_path) return;
    const existing = bySlot.get(cartId(page, slot));
    const button: CartButton = {
      page,
      slot,
      track_id: track.id ?? null,
      file_path: track.file_path,
      label: (track.title ?? "").slice(0, 40),
      color: existing?.color ?? "",
      gain_db: existing?.gain_db ?? 0,
      mode: existing?.mode ?? "Fire",
      duck: existing?.duck ?? false,
    };
    try {
      await live.setButton(bank.id, button);
      await reloadBank(bank.id);
      setNotice(null);
    } catch (err) {
      setNotice(String(err));
    }
  };

  const ctxButton = ctx ? bySlot.get(cartId(page, ctx.slot)) : undefined;
  const ctxItems = (): ContextEntry[] => [
    {
      label: t("live.editButton"),
      action: () => { if (ctx) setEditing({ slot: ctx.slot, button: ctxButton ?? null }); },
    },
    {
      label: t("live.prelisten"), disabled: !ctxButton,
      action: () => {
        if (!ctxButton) return;
        preview.playPath(ctxButton.file_path, 0).catch((e) => setNotice(String(e)));
      },
    },
    "separator",
    {
      label: t("live.clearButton"), danger: true, disabled: !ctxButton,
      action: () => { if (ctx) void clearButton(ctx.slot); },
    },
  ];

  const openButton = (slot: number, button: CartButton | undefined) => {
    if (!button) {
      if (!bank) {
        setNotice(t("live.noBank"));
        return;
      }
      setEditing({ slot, button: null });
      return;
    }
    void fire(button);
  };

  return (
    <div style={S.wrap}>
      <div style={S.toolbar}>
        <select style={S.select} value={activeId ?? ""} aria-label={t("live.bank")}
          disabled={banks.length === 0} onChange={(e) => { void selectBank(Number(e.target.value)); }}>
          {banks.length === 0 && <option value="">{t("live.noBank")}</option>}
          {banks.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
        </select>
        <button style={S.btn} onClick={() => { void createBank(); }}>{t("live.newBank")}</button>
        <button style={S.btn} onClick={() => { void renameBank(); }} disabled={!bank}>{t("live.renameBank")}</button>
        <button style={S.danger} onClick={() => { void deleteBank(); }} disabled={!bank}>{t("live.deleteBank")}</button>
        <button style={S.stopAll} title="Esc" onClick={() => { live.stop(null).catch((e) => setNotice(String(e))); }}>
          {t("live.stopAll")}
        </button>
      </div>

      {notice && <div style={S.notice}>{notice}</div>}

      {loading && !bank && <div style={S.noBank}>{t("library.loading")}</div>}

      {!loading && banks.length === 0 && (
        <div style={S.noBank}>
          <span>{t("live.noBank")}</span>
          <button style={S.btn} onClick={() => { void createBank(); }}>{t("live.newBank")}</button>
        </div>
      )}

      <div style={S.pages} role="tablist" aria-label={t("live.bank")}>
        {Array.from({ length: CART_PAGES }, (_, i) => (
          <button key={i} role="tab" aria-selected={page === i}
            style={{ ...S.page, ...(page === i ? S.pageOn : {}) }}
            onClick={() => setPage(i)}>
            {t("live.page", { n: i + 1 })}
          </button>
        ))}
      </div>

      <div style={{ ...S.grid, gridTemplateColumns: `repeat(${CART_COLUMNS}, minmax(0, 1fr))` }}>
        {Array.from({ length: CART_SLOTS }, (_, slot) => {
          const button = bySlot.get(cartId(page, slot));
          const cart = playing.get(cartId(page, slot));
          const loop = button?.mode === "Loop";
          const id = cartId(page, slot);
          // A Fire button pressed again starts over: more time left than first seen.
          if (cart?.left_ms != null && !(cart.left_ms <= (firstLeft.current[id] ?? -1))) firstLeft.current[id] = cart.left_ms;
          if (!cart) delete firstLeft.current[id];
          const duration = button ? durations[button.file_path] ?? firstLeft.current[id] : undefined;
          const elapsed = cart && cart.left_ms != null && duration
            ? Math.max(0, Math.min(1, 1 - cart.left_ms / duration))
            : loop ? 1 : 0;
          const leftSeconds = cart && cart.left_ms != null ? Math.ceil(cart.left_ms / 1000) : null;
          const key = slot < CART_KEYS.length ? CART_KEYS[slot].toUpperCase() : null;
          return (
            <button key={slot} data-cart-slot={slot}
              style={{
                ...S.cell,
                ...(button ? { background: button.color || "var(--bg-panel)" } : S.cellEmpty),
                ...(cart ? S.cellPlaying : {}),
              }}
              title={button ? button.label : t("live.emptyButton")}
              onClick={() => openButton(slot, button)}
              onContextMenu={(e) => { e.preventDefault(); setCtx({ x: e.clientX, y: e.clientY, slot }); }}
              onDragOver={(e) => { e.preventDefault(); e.dataTransfer.dropEffect = "copy"; }}
              onDrop={(e) => { void dropTrack(e, slot); }}>
              {key && <span style={S.keyHint}>{key}</span>}
              <span style={S.label}>
                {button ? button.label : <span style={S.emptyLabel} aria-hidden>+</span>}
              </span>
              <span style={S.bottom}>
                <span style={S.mode}>
                  {button?.mode === "Loop" ? "↻" : button?.mode === "Toggle" ? "⏯" : ""}
                </span>
                {cart && (
                  <span style={S.left}>
                    {cart.left_ms == null ? "∞" : leftSeconds != null ? t("live.secondsLeft", { s: leftSeconds }) : ""}
                  </span>
                )}
              </span>
              {cart && (
                <span style={S.progress}>
                  <span style={{ ...S.fill, display: "block", width: `${(loop ? 1 : elapsed) * 100}%` }} />
                </span>
              )}
            </button>
          );
        })}
      </div>

      {ctx && (
        <ContextMenu x={ctx.x} y={ctx.y} title={ctxButton?.label ?? t("live.emptyButton")}
          items={ctxItems()} onClose={() => setCtx(null)} />
      )}

      {editing && bank && (
        <CartButtonDialog bankId={bank.id} page={page} slot={editing.slot} button={editing.button}
          onClose={() => setEditing(null)}
          onSaved={() => { void reloadBank(bank.id); }} />
      )}
    </div>
  );
}
