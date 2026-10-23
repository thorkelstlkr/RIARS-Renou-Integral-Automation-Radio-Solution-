import { useState, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";

interface Levels {
  peak_left: number;
  peak_right: number;
  clipped: number;
}

const MIN_DB = -60;
const DECAY_DB_PER_S = 24; // bars fall like a PPM meter
const HOLD_MS = 1500; // peak marker
const CLIP_MS = 2000; // clip light
const SCALE = [0, -3, -6, -12, -24, -48];

const toDb = (linear: number) => (linear > 0 ? Math.max(MIN_DB, 20 * Math.log10(linear)) : MIN_DB);
const toFraction = (db: number) => (db - MIN_DB) / -MIN_DB;
const fmtDb = (db: number) => (db <= MIN_DB ? "-∞" : db.toFixed(1));

interface Side { db: number; holdDb: number; holdUntil: number }

export default function VuMeter() {
  const { t } = useTranslation();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const sides = useRef<Side[]>([
    { db: MIN_DB, holdDb: MIN_DB, holdUntil: 0 },
    { db: MIN_DB, holdDb: MIN_DB, holdUntil: 0 },
  ]);
  const [readout, setReadout] = useState<[number, number]>([MIN_DB, MIN_DB]);
  const [clipUntil, setClipUntil] = useState(0);
  const [now, setNow] = useState(0);

  // Levels arrive as events (only while there is sound); the fall-off is animated
  // with requestAnimationFrame, which stops once the bars are down.
  useEffect(() => {
    let frame = 0;
    let last = performance.now();
    const animate = () => {
      const t = performance.now();
      const fall = (DECAY_DB_PER_S * (t - last)) / 1000;
      last = t;
      let busy = false;
      for (const side of sides.current) {
        side.db = Math.max(MIN_DB, side.db - fall);
        if (t > side.holdUntil) side.holdDb = Math.max(side.db, side.holdDb - fall);
        busy ||= side.db > MIN_DB || side.holdDb > MIN_DB;
      }
      setReadout([sides.current[0].holdDb, sides.current[1].holdDb]);
      setNow(t);
      draw();
      frame = busy ? requestAnimationFrame(animate) : 0;
    };
    const kick = () => { if (!frame) { last = performance.now(); frame = requestAnimationFrame(animate); } };
    const unlisten = listen<Levels>("levels", (e) => {
      const t = performance.now();
      [e.payload.peak_left, e.payload.peak_right].forEach((peak, i) => {
        const side = sides.current[i];
        side.db = Math.max(toDb(peak), side.db); // rise at once
        if (side.db >= side.holdDb) {
          side.holdDb = side.db;
          side.holdUntil = t + HOLD_MS;
        }
      });
      if (e.payload.clipped > 0) setClipUntil(t + CLIP_MS);
      kick();
    });
    draw();
    return () => { unlisten.then((f) => f()); cancelAnimationFrame(frame); };
  }, []);

  const draw = () => {
    const c = canvasRef.current;
    const ctx = c?.getContext("2d");
    if (!c || !ctx) return;
    const w = c.width, h = c.height, labelW = 24, bw = (w - labelW - 12) / 2;
    ctx.clearRect(0, 0, w, h);
    sides.current.forEach((side, i) => drawBar(ctx, labelW + 4 + i * (bw + 4), bw, h, side));
    ctx.fillStyle = "#8b949e";
    ctx.font = "9px monospace";
    ctx.textAlign = "right";
    for (const db of SCALE) {
      const y = h - toFraction(db) * h;
      ctx.fillText(String(db), labelW, Math.min(h - 2, Math.max(8, y + 3)));
    }
  };

  const clipping = now < clipUntil;
  return (
    <div style={{ padding: 12, flex: 1, display: "flex", flexDirection: "column", alignItems: "center" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
        <span style={{ fontSize: 11, fontWeight: 600, color: "var(--text-muted)", textTransform: "uppercase" as const, letterSpacing: 1 }}>
          {t("vuMeter.title")}
        </span>
        <span data-testid="clip" title={t("vuMeter.clipHint")} style={{
          fontSize: 10, fontWeight: 700, padding: "1px 5px", borderRadius: 3,
          background: clipping ? "var(--red)" : "var(--bg-input)", color: clipping ? "#fff" : "var(--text-muted)",
        }}>{t("vuMeter.clip")}</span>
      </div>
      <canvas ref={canvasRef} width={200} height={140} style={{ width: "100%", maxWidth: 200 }} />
      <div style={{ display: "flex", justifyContent: "space-around", width: "100%", maxWidth: 200, marginTop: 8, fontSize: 11, fontFamily: "var(--font-mono)", color: "var(--text-secondary)" }}>
        <span data-testid="db-left">L {fmtDb(readout[0])} dB</span>
        <span data-testid="db-right">R {fmtDb(readout[1])} dB</span>
      </div>
    </div>
  );
}

function drawBar(ctx: CanvasRenderingContext2D, x: number, w: number, h: number, side: Side) {
  const segs = 30, gap = 1, segH = (h - gap * (segs - 1)) / segs;
  const level = toFraction(side.db);
  const hold = toFraction(side.holdDb);
  for (let i = 0; i < segs; i++) {
    const y = h - (i + 1) * (segH + gap);
    const r = (i + 1) / segs;
    const db = MIN_DB + r * -MIN_DB;
    const lit = r <= level || (hold > 0 && Math.abs(r - hold) < 1 / segs);
    ctx.fillStyle = lit ? (db > -3 ? "#f85149" : db > -12 ? "#d29922" : "#3fb950") : "#21262d";
    ctx.fillRect(x, y, w, segH);
  }
}
