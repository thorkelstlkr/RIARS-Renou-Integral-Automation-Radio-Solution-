import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  analysis,
  type AnalysisStats, type BatchScope, type BatchStatus, type CueDetector,
} from "./api";

const S: Record<string, React.CSSProperties> = {
  panel: { padding: "10px 16px 16px", display: "flex", flexDirection: "column", gap: 8 },
  head: { display: "flex", alignItems: "center", gap: 10 },
  title: {
    fontSize: 11, fontWeight: 700, textTransform: "uppercase" as const, letterSpacing: 0.5,
    color: "var(--text-muted)", flex: 1,
  },
  stats: { display: "flex", flexWrap: "wrap" as const, gap: "2px 16px", fontSize: 12, color: "var(--text-secondary)" },
  buttons: { display: "flex", flexWrap: "wrap" as const, gap: 8 },
  btn: { padding: "4px 10px", borderRadius: "var(--radius)", fontSize: 12, background: "var(--bg-panel)" },
  progressWrap: { display: "flex", flexDirection: "column", gap: 3 },
  bar: {
    height: 8, background: "var(--bg-input)", border: "1px solid var(--border)",
    borderRadius: 4, overflow: "hidden",
  },
  fill: { height: "100%", background: "var(--accent)", transition: "width 0.2s" },
  progressInfo: {
    display: "flex", flexWrap: "wrap" as const, gap: "2px 14px", fontSize: 11, color: "var(--text-muted)",
  },
  current: {
    fontFamily: "var(--font-mono)", fontSize: 11, color: "var(--text-secondary)",
    overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const,
  },
  detector: { borderTop: "1px solid var(--border)", paddingTop: 8, display: "flex", flexDirection: "column", gap: 6 },
  radios: { display: "flex", flexWrap: "wrap" as const, gap: 16 },
  radio: { display: "flex", alignItems: "center", gap: 6, fontSize: 12 },
  fields: { display: "flex", flexWrap: "wrap" as const, gap: 8, alignItems: "flex-end" },
  field: { display: "flex", flexDirection: "column", gap: 3, fontSize: 11, color: "var(--text-secondary)" },
  input: { fontSize: 13 },
  textarea: { fontSize: 12, minWidth: 240, resize: "vertical" as const, fontFamily: "var(--font-mono)" },
  hint: { fontSize: 11, color: "var(--text-muted)" },
  error: { fontSize: 12, color: "var(--red)", wordBreak: "break-word" as const },
  notice: { fontSize: 12, color: "var(--green)" },
};

const splitArgs = (text: string) => text.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);

/** Batch analysis and detector settings, shown at the bottom of the Categories tab. */
export default function AnalysisPanel({ categoryId }: { categoryId?: number }) {
  const { t } = useTranslation();
  const [stats, setStats] = useState<AnalysisStats | null>(null);
  const [batch, setBatch] = useState<BatchStatus | null>(null);
  const [running, setRunning] = useState(false);
  const [mode, setMode] = useState<"builtin" | "external">("builtin");
  const [program, setProgram] = useState("");
  const [args, setArgs] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const refreshStats = useCallback(() => {
    analysis.stats().then(setStats).catch((e) => setError(String(e)));
  }, []);

  const applyDetector = (detector: CueDetector) => {
    if (typeof detector === "string") {
      setMode("builtin");
    } else {
      setMode("external");
      setProgram(detector.Command.program);
      setArgs(detector.Command.args.join("\n"));
    }
  };

  useEffect(() => {
    let alive = true;
    analysis.stats().then((value) => { if (alive) setStats(value); }).catch((e) => { if (alive) setError(String(e)); });
    analysis.batchStatus().then((value) => {
      if (alive) { setBatch(value); setRunning(value.running); }
    }).catch(() => {});
    analysis.getDetector().then((value) => {
      if (alive) applyDetector(value);
    }).catch((e) => { if (alive) setError(String(e)); });
    return () => { alive = false; };
  }, []);

  // Poll only while the batch runs.
  useEffect(() => {
    if (!running) return;
    let alive = true;
    const id = setInterval(async () => {
      try {
        const value = await analysis.batchStatus();
        if (!alive) return;
        setBatch(value);
        if (!value.running) {
          setRunning(false);
          refreshStats();
        }
      } catch (e) {
        if (alive) setError(String(e));
      }
    }, 1000);
    return () => { alive = false; clearInterval(id); };
  }, [running, refreshStats]);

  const start = async (scope: BatchScope, reanalyze: boolean) => {
    setError(null);
    setNotice(null);
    try {
      await analysis.batchStart(scope, reanalyze);
      const value = await analysis.batchStatus();
      setBatch(value);
      setRunning(value.running);
    } catch (e) {
      setError(String(e));
    }
  };

  const reanalyzeAll = () => {
    if (!window.confirm(t("analysis.panel.confirmReanalyse"))) return;
    void start("Library", true);
  };

  const cancel = async () => {
    try {
      await analysis.batchCancel();
      const value = await analysis.batchStatus();
      setBatch(value);
      setRunning(value.running);
      refreshStats();
    } catch (e) {
      setError(String(e));
    }
  };

  const saveDetector = async (detector: CueDetector) => {
    try {
      const stored = await analysis.setDetector(detector);
      applyDetector(stored);
      setError(null);
      setNotice(t("analysis.panel.detectorSaved"));
    } catch (e) {
      setError(String(e));
    }
  };

  const chooseMode = (next: "builtin" | "external") => {
    setMode(next);
    if (next === "builtin") void saveDetector("Builtin");
  };

  const applyExternal = () => {
    void saveDetector({ Command: { program, args: splitArgs(args) } });
  };

  const percent = batch && batch.total > 0 ? Math.round((batch.done / batch.total) * 100) : 0;
  const showProgress = batch != null && (batch.running || batch.done > 0);

  return (
    <div style={S.panel}>
      <div style={S.head}>
        <span style={S.title}>{t("analysis.panel.title")}</span>
        {stats && (
          <span style={S.stats}>
            <span>{t("analysis.panel.statsAnalysed", { analyzed: stats.analyzed, total: stats.tracks })}</span>
            <span>{t("analysis.panel.statsManual", { count: stats.manual })}</span>
            <span>{t("analysis.panel.statsWithIntro", { count: stats.with_intro })}</span>
            <span>{t("analysis.panel.statsLowConfidence", { count: stats.low_confidence })}</span>
          </span>
        )}
      </div>

      <div style={S.buttons}>
        <button style={S.btn} disabled={running} onClick={() => start("Library", false)}>
          {t("analysis.panel.analyseLibrary")}
        </button>
        <button style={S.btn} disabled={running} onClick={reanalyzeAll}>
          {t("analysis.panel.reanalyseAll")}
        </button>
        <button style={S.btn} disabled={running || categoryId == null}
          onClick={() => categoryId != null && start({ Category: { category_id: categoryId } }, false)}>
          {t("analysis.panel.analyseCategory")}
        </button>
        <button style={S.btn} disabled={!running} onClick={cancel}>
          {t("menu.cancel")}
        </button>
      </div>

      {showProgress && batch && (
        <div style={S.progressWrap}>
          <div style={S.bar} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent}>
            <div style={{ ...S.fill, width: `${percent}%` }} />
          </div>
          <div style={S.progressInfo}>
            <span>{t("analysis.panel.progress", { done: batch.done, total: batch.total })}</span>
            {batch.running && <span>{t("analysis.panel.running")}</span>}
            <span>{t("analysis.panel.failed", { count: batch.failed })}</span>
            <span>{t("analysis.panel.keptManual", { count: batch.kept_manual })}</span>
          </div>
          {batch.current && (
            <div style={S.current} title={batch.current}>
              {t("analysis.panel.current", { file: batch.current })}
            </div>
          )}
        </div>
      )}

      {error && <div style={S.error}>{error}</div>}
      {notice && <div style={S.notice}>{notice}</div>}

      <div style={S.detector}>
        <div style={S.radios}>
          <span style={S.hint}>{t("analysis.panel.detector")}</span>
          <label style={S.radio}>
            <input type="radio" name="cue-detector" checked={mode === "builtin"}
              onChange={() => chooseMode("builtin")} />
            {t("analysis.panel.builtin")}
          </label>
          <label style={S.radio}>
            <input type="radio" name="cue-detector" checked={mode === "external"}
              onChange={() => chooseMode("external")} />
            {t("analysis.panel.external")}
          </label>
        </div>

        {mode === "external" && (
          <div style={S.fields}>
            <label style={{ ...S.field, flex: 1 }}>
              {t("analysis.panel.program")}
              <input style={S.input} value={program} onChange={(e) => setProgram(e.target.value)} />
            </label>
            <label style={S.field}>
              {t("analysis.panel.args")}
              <textarea style={S.textarea} rows={2} value={args} onChange={(e) => setArgs(e.target.value)} />
            </label>
            <button style={S.btn} onClick={applyExternal}>{t("analysis.panel.apply")}</button>
          </div>
        )}

        <div style={S.hint}>{t("analysis.panel.agentNote")}</div>
      </div>
    </div>
  );
}
