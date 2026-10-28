import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import Markdown from "./Markdown";
import { SECTIONS, searchHelp, sectionTitle, topicsFor } from "./help";

interface HelpDialogProps {
  open: boolean;
  topic?: string;
  onClose: () => void;
}

const S: Record<string, React.CSSProperties> = {
  overlay: {
    position: "fixed",
    inset: 0,
    background: "rgba(0, 0, 0, 0.65)",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    padding: 24,
    zIndex: 1000,
  },
  dialog: {
    width: "min(1120px, 96vw)",
    height: "min(820px, 92vh)",
    background: "var(--bg-panel)",
    border: "1px solid var(--border)",
    borderRadius: 10,
    boxShadow: "0 24px 64px rgba(0, 0, 0, 0.55)",
    display: "flex",
    overflow: "hidden",
  },
  left: {
    width: 300,
    minWidth: 260,
    flexShrink: 0,
    borderRight: "1px solid var(--border)",
    background: "var(--bg-secondary)",
    display: "flex",
    flexDirection: "column",
    minHeight: 0,
  },
  leftHead: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    padding: "12px 14px",
    borderBottom: "1px solid var(--border)",
  },
  title: { fontSize: 15, fontWeight: 700, color: "var(--accent)", letterSpacing: 0.2 },
  close: {
    width: 28,
    height: 28,
    borderRadius: 6,
    fontSize: 18,
    lineHeight: 1,
    color: "var(--text-secondary)",
  },
  searchWrap: { padding: "10px 12px", borderBottom: "1px solid var(--border)" },
  search: { width: "100%", fontSize: 13 },
  contentsLabel: {
    padding: "10px 14px 4px",
    fontSize: 10,
    fontWeight: 700,
    letterSpacing: 1,
    textTransform: "uppercase",
    color: "var(--text-muted)",
  },
  nav: { flex: 1, minHeight: 0, overflowY: "auto", padding: "4px 8px 16px" },
  sectionBtn: {
    width: "100%",
    display: "flex",
    alignItems: "center",
    gap: 6,
    padding: "6px 8px",
    borderRadius: 6,
    color: "var(--text-secondary)",
    fontSize: 11,
    fontWeight: 700,
    letterSpacing: 0.6,
    textTransform: "uppercase",
    textAlign: "left",
  },
  sectionBtnActive: { color: "var(--accent)" },
  chevron: { width: 10, fontSize: 9, color: "var(--text-muted)" },
  topicBtn: {
    width: "100%",
    display: "block",
    padding: "5px 10px 5px 22px",
    borderRadius: 6,
    textAlign: "left",
    fontSize: 13,
    color: "var(--text-secondary)",
    lineHeight: 1.35,
  },
  topicBtnActive: {
    background: "rgba(233, 69, 96, 0.12)",
    color: "var(--text-primary)",
    boxShadow: "inset 2px 0 0 var(--accent)",
  },
  results: { listStyle: "none", margin: 0, padding: 0 },
  result: {
    width: "100%",
    display: "block",
    textAlign: "left",
    padding: "8px 10px",
    borderRadius: 6,
    marginBottom: 2,
    color: "var(--text-secondary)",
  },
  resultActive: { background: "var(--bg-input)", color: "var(--text-primary)" },
  resultTitle: { fontSize: 13, fontWeight: 600, color: "var(--text-primary)" },
  resultSection: {
    fontSize: 10,
    textTransform: "uppercase",
    letterSpacing: 0.6,
    color: "var(--text-muted)",
    marginTop: 1,
  },
  snippet: { fontSize: 11.5, lineHeight: 1.45, marginTop: 3, color: "var(--text-secondary)" },
  mark: {
    background: "rgba(233, 69, 96, 0.35)",
    color: "var(--text-primary)",
    borderRadius: 2,
    padding: "0 1px",
  },
  empty: { padding: "16px 12px", fontSize: 12, color: "var(--text-muted)" },
  right: {
    flex: 1,
    minWidth: 0,
    display: "flex",
    flexDirection: "column",
    background: "var(--bg-primary)",
  },
  rightHead: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    padding: "10px 16px",
    borderBottom: "1px solid var(--border)",
    background: "var(--bg-panel)",
  },
  navBtn: {
    padding: "4px 9px",
    borderRadius: 6,
    border: "1px solid var(--border)",
    background: "var(--bg-input)",
    color: "var(--text-secondary)",
    fontSize: 12,
    whiteSpace: "nowrap",
  },
  navBtnDisabled: { opacity: 0.4, cursor: "default" },
  crumb: {
    flex: 1,
    minWidth: 0,
    display: "flex",
    alignItems: "center",
    gap: 6,
    fontSize: 12,
    color: "var(--text-muted)",
    overflow: "hidden",
  },
  crumbSection: { color: "var(--text-secondary)", whiteSpace: "nowrap" },
  crumbSep: { color: "var(--text-muted)" },
  crumbTitle: {
    color: "var(--text-primary)",
    fontWeight: 600,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  scroll: { flex: 1, minHeight: 0, overflowY: "auto" },
  article: { maxWidth: 760, width: "100%", margin: "0 auto", padding: "20px 32px 48px" },
  fallback: {
    margin: "0 0 16px",
    padding: "8px 10px",
    borderRadius: 6,
    border: "1px solid var(--border)",
    background: "var(--bg-secondary)",
    color: "var(--text-secondary)",
    fontSize: 12,
    lineHeight: 1.4,
  },
  pager: {
    display: "flex",
    justifyContent: "space-between",
    gap: 12,
    marginTop: 36,
    paddingTop: 16,
    borderTop: "1px solid var(--border)",
  },
  pagerBtn: {
    display: "flex",
    flexDirection: "column",
    gap: 2,
    padding: "8px 10px",
    borderRadius: 6,
    border: "1px solid var(--border)",
    background: "var(--bg-secondary)",
    color: "var(--text-secondary)",
    maxWidth: "48%",
    textAlign: "left",
  },
  pagerLabel: {
    fontSize: 10,
    textTransform: "uppercase",
    letterSpacing: 0.6,
    color: "var(--text-muted)",
  },
  pagerTitle: {
    fontSize: 13,
    color: "var(--text-primary)",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
};

function foldText(value: string): string {
  return value.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase();
}

function queryWords(query: string): string[] {
  return Array.from(new Set(foldText(query).split(/[^\p{L}\p{N}]+/u).filter(Boolean)));
}

function highlight(text: string, words: string[]): ReactNode {
  if (words.length === 0) return text;

  const folded = foldText(text);
  const ranges: Array<[number, number]> = [];
  for (const word of words) {
    let from = 0;
    while (from < folded.length) {
      const index = folded.indexOf(word, from);
      if (index < 0) break;
      ranges.push([index, index + word.length]);
      from = index + word.length;
    }
  }

  if (ranges.length === 0) return text;

  ranges.sort((a, b) => a[0] - b[0]);
  const merged: Array<[number, number]> = [];
  for (const range of ranges) {
    const last = merged[merged.length - 1];
    if (last && range[0] <= last[1]) last[1] = Math.max(last[1], range[1]);
    else merged.push([range[0], range[1]]);
  }

  const nodes: ReactNode[] = [];
  let position = 0;
  merged.forEach(([rawStart, rawEnd], index) => {
    const start = Math.max(0, Math.min(rawStart, text.length));
    const end = Math.max(start, Math.min(rawEnd, text.length));
    if (start > position) nodes.push(text.slice(position, start));
    nodes.push(<mark key={index} style={S.mark}>{text.slice(start, end)}</mark>);
    position = end;
  });
  if (position < text.length) nodes.push(text.slice(position));
  return nodes;
}

export default function HelpDialog({ open, topic, onClose }: HelpDialogProps) {
  const { t, i18n } = useTranslation();
  const lang = (i18n.resolvedLanguage || i18n.language || "en").toLowerCase().split("-")[0];
  const topics = useMemo(() => topicsFor(lang), [lang]);
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [query, setQuery] = useState("");
  const [activeResult, setActiveResult] = useState(0);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const wasOpen = useRef(false);

  const current = historyIndex >= 0 ? history[historyIndex] ?? null : null;
  const currentTopic = current ? topics.find((item) => item.id === current) : undefined;
  const currentIndex = currentTopic
    ? topics.findIndex((item) => item.id === currentTopic.id)
    : -1;
  const previousTopic = currentIndex > 0 ? topics[currentIndex - 1] : undefined;
  const nextTopic =
    currentIndex >= 0 && currentIndex < topics.length - 1
      ? topics[currentIndex + 1]
      : undefined;

  const words = useMemo(() => queryWords(query), [query]);
  const results = useMemo(() => searchHelp(lang, query), [lang, query]);
  const searching = query.trim().length > 0;
  const safeActive = results.length > 0 ? Math.min(activeResult, results.length - 1) : 0;

  const sections = useMemo(
    () =>
      SECTIONS.map((id) => ({
        id,
        topics: topics.filter((item) => item.section === id),
      })).filter((section) => section.topics.length > 0),
    [topics],
  );

  useEffect(() => {
    if (!open) {
      wasOpen.current = false;
      return;
    }
    if (wasOpen.current) return;
    wasOpen.current = true;
    const desired =
      topic && topics.some((item) => item.id === topic)
        ? topic
        : topics.some((item) => item.id === "welcome")
          ? "welcome"
          : topics[0]?.id;
    setHistory(desired ? [desired] : []);
    setHistoryIndex(desired ? 0 : -1);
    setQuery("");
    setActiveResult(0);
  }, [open, topic, topics]);

  useEffect(() => {
    if (!open || !current) return;
    if (topics.some((item) => item.id === current)) return;
    const desired = topics.some((item) => item.id === "welcome")
      ? "welcome"
      : topics[0]?.id;
    setHistory(desired ? [desired] : []);
    setHistoryIndex(desired ? 0 : -1);
  }, [open, current, topics]);

  useEffect(() => {
    if (!open) return;
    const timer = window.setTimeout(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    }, 0);
    return () => window.clearTimeout(timer);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  useEffect(() => {
    if (open && scrollRef.current) scrollRef.current.scrollTop = 0;
  }, [open, current]);

  useEffect(() => {
    setActiveResult(0);
  }, [query]);

  const go = (id: string) => {
    if (!id || id === current) return;
    const next = history.slice(0, historyIndex + 1);
    next.push(id);
    setHistory(next);
    setHistoryIndex(next.length - 1);
  };

  const back = () => setHistoryIndex((index) => Math.max(0, index - 1));
  const forward = () =>
    setHistoryIndex((index) => Math.min(history.length - 1, index + 1));

  const onSearchKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (!searching) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveResult((index) => Math.min(index + 1, Math.max(0, results.length - 1)));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveResult((index) => Math.max(0, index - 1));
    } else if (event.key === "Enter") {
      const result = results[safeActive];
      if (result) {
        event.preventDefault();
        go(result.topic.id);
      }
    }
  };

  const onDialogKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Tab") return;
    const root = dialogRef.current;
    if (!root) return;

    const focusable = Array.from(
      root.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ),
    ).filter(
      (element) =>
        element.getClientRects().length > 0 || element === document.activeElement,
    );

    if (focusable.length === 0) {
      event.preventDefault();
      return;
    }

    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  const toggleSection = (id: string) => {
    setCollapsed((value) => ({ ...value, [id]: !value[id] }));
  };

  if (!open) return null;

  return (
    <div
      style={S.overlay}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label={t("help.title")}
        style={S.dialog}
        onKeyDown={onDialogKeyDown}
      >
        <aside style={S.left}>
          <div style={S.leftHead}>
            <span style={S.title}>{t("help.title")}</span>
            <button
              type="button"
              style={S.close}
              onClick={onClose}
              aria-label={t("help.close")}
              title={t("help.close")}
            >
              ×
            </button>
          </div>
          <div style={S.searchWrap}>
            <input
              ref={inputRef}
              type="search"
              role="searchbox"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={onSearchKeyDown}
              placeholder={t("help.placeholder")}
              aria-label={t("help.search")}
              style={S.search}
            />
          </div>
          {searching ? (
            <div style={S.nav} role="region" aria-label={t("help.search")}>
              {results.length === 0 ? (
                <div style={S.empty}>{t("help.noResults")}</div>
              ) : (
                <ul style={S.results} role="listbox" aria-label={t("help.search")}>
                  {results.map((result, index) => (
                    <li key={result.topic.id} role="presentation">
                      <button
                        type="button"
                        role="option"
                        aria-selected={index === safeActive}
                        style={{
                          ...S.result,
                          ...(index === safeActive ? S.resultActive : {}),
                        }}
                        onMouseEnter={() => setActiveResult(index)}
                        onFocus={() => setActiveResult(index)}
                        onClick={() => go(result.topic.id)}
                      >
                        <div style={S.resultTitle}>{result.topic.title}</div>
                        <div style={S.resultSection}>
                          {sectionTitle(result.topic.section, t)}
                        </div>
                        <div style={S.snippet}>
                          {highlight(result.snippet, words)}
                        </div>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          ) : (
            <div style={S.nav}>
              <div style={S.contentsLabel}>{t("help.contents")}</div>
              {sections.map((section) => {
                const isCollapsed = collapsed[section.id] === true;
                const hasCurrent = section.topics.some((item) => item.id === current);
                return (
                  <div key={section.id}>
                    <button
                      type="button"
                      style={{
                        ...S.sectionBtn,
                        ...(hasCurrent ? S.sectionBtnActive : {}),
                      }}
                      aria-expanded={!isCollapsed}
                      onClick={() => toggleSection(section.id)}
                    >
                      <span style={S.chevron} aria-hidden>
                        {isCollapsed ? "▶" : "▼"}
                      </span>
                      <span>{sectionTitle(section.id, t)}</span>
                    </button>
                    {!isCollapsed &&
                      section.topics.map((item) => (
                        <button
                          key={item.id}
                          type="button"
                          style={{
                            ...S.topicBtn,
                            ...(item.id === current ? S.topicBtnActive : {}),
                          }}
                          aria-current={item.id === current ? "page" : undefined}
                          onClick={() => go(item.id)}
                        >
                          {item.title}
                        </button>
                      ))}
                  </div>
                );
              })}
            </div>
          )}
        </aside>
        <section style={S.right}>
          <div style={S.rightHead}>
            <button
              type="button"
              style={{
                ...S.navBtn,
                ...(historyIndex <= 0 ? S.navBtnDisabled : {}),
              }}
              disabled={historyIndex <= 0}
              onClick={back}
              aria-label={t("help.back")}
              title={t("help.back")}
            >
              ← {t("help.back")}
            </button>
            <button
              type="button"
              style={{
                ...S.navBtn,
                ...(historyIndex >= history.length - 1 ? S.navBtnDisabled : {}),
              }}
              disabled={historyIndex >= history.length - 1}
              onClick={forward}
              aria-label={t("help.forward")}
              title={t("help.forward")}
            >
              {t("help.forward")} →
            </button>
            <div style={S.crumb}>
              {currentTopic ? (
                <>
                  <span style={S.crumbSection}>
                    {sectionTitle(currentTopic.section, t)}
                  </span>
                  <span style={S.crumbSep} aria-hidden>
                    ›
                  </span>
                  <span style={S.crumbTitle}>{currentTopic.title}</span>
                </>
              ) : null}
            </div>
          </div>
          <div ref={scrollRef} style={S.scroll}>
            <article style={S.article}>
              {currentTopic ? (
                <>
                  {currentTopic.fallback ? (
                    <div style={S.fallback}>{t("help.fallback")}</div>
                  ) : null}
                  <Markdown source={currentTopic.body} onLink={go} />
                  {previousTopic || nextTopic ? (
                    <nav style={S.pager} aria-label={t("help.title")}>
                      {previousTopic ? (
                        <button
                          type="button"
                          style={S.pagerBtn}
                          onClick={() => go(previousTopic.id)}
                        >
                          <span style={S.pagerLabel}>← {t("help.previous")}</span>
                          <span style={S.pagerTitle}>{previousTopic.title}</span>
                        </button>
                      ) : (
                        <span />
                      )}
                      {nextTopic ? (
                        <button
                          type="button"
                          style={{
                            ...S.pagerBtn,
                            textAlign: "right",
                            alignItems: "flex-end",
                          }}
                          onClick={() => go(nextTopic.id)}
                        >
                          <span style={S.pagerLabel}>{t("help.next")} →</span>
                          <span style={S.pagerTitle}>{nextTopic.title}</span>
                        </button>
                      ) : null}
                    </nav>
                  ) : null}
                </>
              ) : null}
            </article>
          </div>
        </section>
      </div>
    </div>
  );
}
