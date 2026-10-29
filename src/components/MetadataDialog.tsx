import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import Dialog from "../menu/Dialog";
import {
  applyItems, changes, initialTicks, metadata, thumbnail, tickKey,
  type Field, type LookupStatus, type MissingCount, type Ticks,
} from "./metadataReview";

const S: Record<string, React.CSSProperties> = {
  intro: { fontSize: 13, color: "var(--text-secondary)", marginBottom: 12, lineHeight: 1.5 },
  keyBox: { border: "1px solid var(--border)", borderRadius: 6, padding: "8px 10px", marginBottom: 12, fontSize: 12 },
  keyRow: { display: "flex", gap: 8, alignItems: "center", marginTop: 6 },
  hint: { fontSize: 12, color: "var(--text-muted)" },
  progress: { height: 6, background: "var(--bg-panel)", borderRadius: 3, overflow: "hidden", margin: "8px 0" },
  bar: { height: "100%", background: "var(--accent)" },
  current: { fontSize: 12, color: "var(--text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  list: { maxHeight: "52vh", overflowY: "auto", border: "1px solid var(--border)", borderRadius: 6 },
  card: { display: "flex", gap: 10, padding: "10px 12px", borderBottom: "1px solid var(--border)" },
  cover: { width: 64, height: 64, flexShrink: 0, borderRadius: 4, background: "var(--bg-panel)", objectFit: "cover" as const },
  cardBody: { flex: 1, minWidth: 0 },
  head: { display: "flex", justifyContent: "space-between", gap: 8, fontSize: 13, fontWeight: 600, marginBottom: 4 },
  file: { fontSize: 11, color: "var(--text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const, marginBottom: 4 },
  badge: { fontSize: 11, fontWeight: 400, color: "var(--text-muted)", whiteSpace: "nowrap" as const },
  row: { display: "grid", gridTemplateColumns: "20px 70px 1fr 14px 1fr", gap: 6, alignItems: "center", fontSize: 12, padding: "2px 0" },
  old: { color: "var(--text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  next: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
  applied: { fontSize: 12, color: "var(--green, #4caf50)" },
  missing: { fontSize: 12, color: "var(--text-muted)", padding: "8px 12px" },
  footer: { display: "flex", alignItems: "center", gap: 8, marginTop: 12, flexWrap: "wrap" as const },
  spacer: { flex: 1 },
  btn: { padding: "6px 14px", borderRadius: "var(--radius)", background: "var(--bg-panel)", fontSize: 13 },
  primary: { padding: "6px 14px", borderRadius: "var(--radius)", background: "var(--accent)", color: "#fff", fontSize: 13 },
  error: { color: "var(--red)", fontSize: 12, marginTop: 8, whiteSpace: "pre-wrap" as const },
  notice: { color: "var(--text-secondary)", fontSize: 12, marginTop: 8 },
};

/** Seconds per track at MusicBrainz's pace (a search, the recording, sometimes genres). */
const SECONDS_PER_TRACK = 3;

/**
 * Library ▸ Complete metadata…: looks the tracks up in MusicBrainz (and AcoustID, with
 * a key), shows what it found next to what the track has, and applies only what the
 * operator ticks. `trackIds` empty means every track with gaps.
 *
 * Nothing starts on its own: the dialog first says what Search will look up (or why
 * there is nothing to look up), and Search is always there.
 */
export default function MetadataDialog({ trackIds, onClose, onApplied }: {
  trackIds: number[]; onClose: () => void; onApplied: () => void;
}) {
  const { t } = useTranslation();
  const [status, setStatus] = useState<LookupStatus | null>(null);
  const [missing, setMissing] = useState<MissingCount | null>(null);
  const [key, setKey] = useState("");
  const [savedKey, setSavedKey] = useState<string | null>(null);
  const [ticks, setTicks] = useState<Ticks>(new Set());
  const [writeFile, setWriteFile] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [noCover, setNoCover] = useState<Set<number>>(new Set());
  // Results already given their first ticks, so a poll never re-ticks what was unticked.
  const ticked = useRef(new Set<number>());

  const refresh = useCallback(async () => {
    const next = await metadata.status();
    setStatus(next);
    const fresh = next.results.filter((p) => !ticked.current.has(p.track_id));
    if (fresh.length > 0) {
      fresh.forEach((p) => ticked.current.add(p.track_id));
      const add = initialTicks(fresh);
      setTicks((prev) => new Set([...prev, ...add]));
    }
    return next;
  }, []);

  const start = useCallback(async (ids: number[]) => {
    setError(null);
    setNotice(null);
    ticked.current.clear();
    setTicks(new Set());
    setNoCover(new Set());
    try {
      await metadata.start(ids);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }, [refresh]);

  // On open: the key, the tracks a whole-library lookup would take, and a lookup
  // already under way (shown, since only one runs at a time).
  useEffect(() => {
    metadata.getKey().then((k) => { setSavedKey(k); setKey(k ?? ""); }).catch(() => {});
    (async () => {
      try {
        if (trackIds.length === 0) setMissing(await metadata.missingCount());
        const now = await metadata.status();
        if (now.running) await refresh();
      } catch (e) { setError(String(e)); }
    })();
  }, [trackIds, refresh]);

  // Poll while the lookup runs.
  useEffect(() => {
    if (!status?.running) return;
    const id = setInterval(() => { refresh().catch(() => {}); }, 1000);
    return () => clearInterval(id);
  }, [status?.running, refresh]);

  const saveKey = async () => {
    try {
      const stored = await metadata.setKey(key);
      setSavedKey(stored);
      setKey(stored ?? "");
      setNotice(stored ? t("metadata.keySaved") : t("metadata.keyCleared"));
    } catch (e) { setError(String(e)); }
  };

  const results = status?.results ?? [];
  const found = useMemo(() => results.filter((p) => p.found && changes(p).length + (p.found.cover_url ? 1 : 0) > 0), [results]);
  const unmatched = useMemo(() => results.filter((p) => !p.found), [results]);
  // A cover the archive turned out not to have (its thumbnail failed) is not sent.
  const items = useMemo(() => {
    const effective = new Set([...ticks].filter((k) => {
      const [id, field] = k.split(":");
      return field !== "cover" || !noCover.has(Number(id));
    }));
    return applyItems(results, effective, writeFile);
  }, [results, ticks, noCover, writeFile]);

  const toggle = (k: string) => setTicks((prev) => {
    const next = new Set(prev);
    if (next.has(k)) next.delete(k); else next.add(k);
    return next;
  });

  const apply = async () => {
    if (items.length === 0) return;
    if (writeFile && !window.confirm(t("metadata.confirmWrite", { count: items.length }))) return;
    setBusy(true);
    setError(null);
    try {
      const outcomes = await metadata.apply(items);
      const ok = outcomes.filter((o) => o.applied).length;
      const failed = outcomes.filter((o) => o.error);
      setNotice(t("metadata.appliedCount", { count: ok }));
      if (failed.length > 0) {
        const name = (id: number) => results.find((p) => p.track_id === id)?.current.title ?? String(id);
        setError(failed.map((o) => `${name(o.track_id)}: ${o.error}`).join("\n"));
      }
      await refresh();
      if (trackIds.length === 0) setMissing(await metadata.missingCount());
      onApplied();
      // The playlist shows album, year and genre from the library.
      window.dispatchEvent(new Event("library-changed"));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const fieldLabel = (f: Field) => t(`metadata.field.${f}`);
  const methodLabel = (m: string) => t(`metadata.method.${m}`);
  const running = status?.running ?? false;
  const started = status !== null && (status.running || status.total > 0);
  // What Search looks up: the selection as it is, or the library's tracks with gaps.
  const count = trackIds.length > 0 ? trackIds.length : missing?.missing ?? 0;
  const minutes = Math.max(1, Math.ceil((count * SECONDS_PER_TRACK) / 60));
  const scope = trackIds.length > 0
    ? t("metadata.scopeSelected", { count, minutes })
    : missing === null
      ? null
      : missing.missing === 0
        ? t("metadata.noneMissing")
        : t("metadata.wholeLibrary", { count, minutes });
  const skipped = trackIds.length === 0 && missing && missing.short > 0
    ? t("metadata.skippedShort", { count: missing.short })
    : null;
  // Why Search can't be pressed, if it can't.
  const blocked = running ? t("metadata.busy") : count === 0 && missing !== null ? t("metadata.nothingToSearch") : null;

  return (
    <Dialog title={t("metadata.title")} width={720} onClose={onClose}>
      <div style={S.keyBox}>
        <div>{savedKey ? t("metadata.fingerprintOn") : t("metadata.fingerprintOff")}</div>
        <div style={S.keyRow}>
          <input style={{ flex: 1, fontSize: 12 }} value={key} placeholder={t("metadata.keyPlaceholder")}
            aria-label={t("metadata.keyLabel")} data-testid="metadata-key"
            onChange={(e) => setKey(e.target.value)} />
          <button style={S.btn} onClick={saveKey} disabled={key.trim() === (savedKey ?? "")}>{t("metadata.saveKey")}</button>
        </div>
        <div style={{ ...S.hint, marginTop: 4 }}>{t("metadata.keyHint")}</div>
      </div>

      {scope && (
        <div style={S.intro} data-testid="metadata-intro">
          {scope}
          {skipped && <div style={{ ...S.hint, marginTop: 4 }}>{skipped}</div>}
        </div>
      )}

      {status && (running || status.total > 0) && (
        <div data-testid="metadata-progress">
          <div style={S.hint}>
            {running
              ? t("metadata.progress", { done: status.done, total: status.total })
              : status.stopped
                ? t("metadata.stopped", { done: status.done, total: status.total })
                : t("metadata.finished", { found: found.length, total: status.total })}
          </div>
          <div style={S.progress}>
            <div style={{ ...S.bar, width: `${status.total ? (status.done / status.total) * 100 : 0}%` }} />
          </div>
          {running && status.current && <div style={S.current}>{status.current}</div>}
        </div>
      )}

      {found.length > 0 && (
        <div style={{ ...S.list, marginTop: 10 }} data-testid="metadata-results">
          {found.map((p) => {
            const f = p.found!;
            const coverKey = tickKey(p.track_id, "cover");
            const hasCover = f.cover_url && !noCover.has(p.track_id);
            return (
              <div key={p.track_id} style={S.card} data-testid="metadata-card">
                {f.cover_url && !noCover.has(p.track_id)
                  ? <img style={S.cover} src={thumbnail(f.cover_url)} alt=""
                      onError={() => setNoCover((prev) => new Set(prev).add(p.track_id))} />
                  : <div style={S.cover} />}
                <div style={S.cardBody}>
                  <div style={S.head}>
                    <span>{p.current.title}{p.current.artist ? ` — ${p.current.artist}` : ""}</span>
                    <span style={S.badge}>
                      {methodLabel(f.method)} · {Math.round(f.confidence * 100)} %
                    </span>
                  </div>
                  <div style={S.file} title={p.current.file_path}>{p.current.file_path}</div>
                  {p.applied
                    ? <div style={S.applied}>{t("metadata.applied")}</div>
                    : (
                      <>
                        {changes(p).map((c) => {
                          const k = tickKey(p.track_id, c.field);
                          return (
                            <label key={c.field} style={S.row}>
                              <input type="checkbox" checked={ticks.has(k)} onChange={() => toggle(k)} />
                              <span>{fieldLabel(c.field)}</span>
                              <span style={S.old} title={c.current}>{c.current || t("metadata.empty")}</span>
                              <span>→</span>
                              <span style={S.next} title={c.proposed}>{c.proposed}</span>
                            </label>
                          );
                        })}
                        {hasCover && (
                          <label style={S.row}>
                            <input type="checkbox" checked={ticks.has(coverKey)} onChange={() => toggle(coverKey)} />
                            <span>{t("metadata.field.cover")}</span>
                            <span style={S.old}>{p.current.cover_path ? t("metadata.hasCover") : t("metadata.empty")}</span>
                            <span>→</span>
                            <span style={S.next}>{t("metadata.coverArchive")}</span>
                          </label>
                        )}
                      </>
                    )}
                </div>
              </div>
            );
          })}
        </div>
      )}

      {!running && unmatched.length > 0 && (
        <details style={{ marginTop: 10 }} data-testid="metadata-unmatched">
          <summary style={S.hint}>{t("metadata.unmatched", { count: unmatched.length })}</summary>
          {unmatched.map((p) => (
            <div key={p.track_id} style={S.missing} title={p.current.file_path}>
              {p.current.title}{p.error ? ` — ${p.error}` : ""}
            </div>
          ))}
          {!status?.fingerprints && <div style={S.missing}>{t("metadata.unmatchedHint")}</div>}
        </details>
      )}

      {error && <div style={S.error} role="alert">{error}</div>}
      {notice && <div style={S.notice} role="status">{notice}</div>}

      <div style={S.footer}>
        {found.length > 0 && (
          <label style={{ fontSize: 12, display: "flex", gap: 6, alignItems: "center" }}>
            <input type="checkbox" checked={writeFile} onChange={(e) => setWriteFile(e.target.checked)} data-testid="metadata-write-file" />
            {t("metadata.writeFile")}
          </label>
        )}
        <span style={S.spacer} />
        {blocked && <span style={S.hint} data-testid="metadata-blocked">{blocked}</span>}
        {running && <button style={S.btn} onClick={() => { metadata.cancel().catch(() => {}); }}>{t("metadata.cancel")}</button>}
        <button style={started || blocked ? S.btn : S.primary} disabled={!!blocked || (trackIds.length === 0 && missing === null)}
          onClick={() => start(trackIds)} data-testid="metadata-start">
          {started ? t("metadata.again") : t("metadata.start")}
        </button>
        {found.length > 0 && (
          <button style={S.primary} disabled={busy || items.length === 0} onClick={apply} data-testid="metadata-apply">
            {t("metadata.apply", { count: items.length })}
          </button>
        )}
        <button style={S.btn} onClick={onClose}>{t("menu.close")}</button>
      </div>
    </Dialog>
  );
}
