import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

export type WaveformMarkId = "cueIn" | "introEnd" | "outroStart" | "cueOut";

export interface WaveformMark {
  id: WaveformMarkId;
  ms: number | null;
  color: string;
  label: string;
}

interface Props {
  peaks: number[];
  durationMs: number;
  marks: WaveformMark[];
  onMove: (id: WaveformMarkId, ms: number) => void;
  positionMs?: number | null;
  height?: number;
}

const RULER = 18;
const STEPS = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600];

const tickLabel = (seconds: number) => {
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
};

/** Mirrored waveform with draggable cue marks and a play-head. */
export default function Waveform({ peaks, durationMs, marks, onMove, positionMs, height = 140 }: Props) {
  const { t } = useTranslation();
  const wrap = useRef<HTMLDivElement>(null);
  const drag = useRef<WaveformMarkId | null>(null);
  const [width, setWidth] = useState(0);

  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    setWidth(el.clientWidth);
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) setWidth(entry.contentRect.width);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const x = (ms: number) => (durationMs > 0 ? (Math.max(0, Math.min(durationMs, ms)) / durationMs) * width : 0);
  const msAt = (clientX: number) => {
    const el = wrap.current;
    if (!el || durationMs <= 0) return 0;
    const rect = el.getBoundingClientRect();
    if (rect.width <= 0) return 0;
    return Math.max(0, Math.min(durationMs, ((clientX - rect.left) / rect.width) * durationMs));
  };
  const moveBy = (id: WaveformMarkId, ms: number, delta: number) => {
    onMove(id, Math.max(0, Math.min(durationMs, ms + delta)));
  };

  const plotHeight = Math.max(20, height - RULER);
  const top = 4;
  const bottom = plotHeight - 2;
  const mid = (top + bottom) / 2;
  const amplitude = Math.max(1, (bottom - top) / 2);
  const barWidth = peaks.length > 0 && width > 0 ? width / peaks.length : 0;

  const markMs = (id: WaveformMarkId) => marks.find((m) => m.id === id)?.ms ?? null;
  const cueIn = markMs("cueIn");
  const introEnd = markMs("introEnd");
  const outroStart = markMs("outroStart");
  const cueOut = markMs("cueOut");

  const seconds = durationMs / 1000;
  const targetTicks = Math.max(2, Math.floor(width / 90));
  const step = STEPS.find((s) => seconds / s <= targetTicks) ?? STEPS[STEPS.length - 1];
  const ticks: number[] = [];
  for (let s = 0; s <= seconds; s += step) ticks.push(s);

  return (
    <div
      ref={wrap}
      role="group"
      aria-label={t("analysis.waveform.label")}
      style={{ position: "relative", width: "100%", height, userSelect: "none" }}
    >
      <svg width={width} height={height} style={{ display: "block" }} aria-hidden="true">
        <rect x={0} y={0} width={width} height={height} fill="var(--bg-input)" />

        {cueIn != null && introEnd != null && introEnd > cueIn && (
          <rect
            x={x(cueIn)}
            y={top}
            width={Math.max(0, x(introEnd) - x(cueIn))}
            height={bottom - top}
            fill="rgba(63,185,80,0.18)"
          />
        )}
        {outroStart != null && cueOut != null && cueOut > outroStart && (
          <rect
            x={x(outroStart)}
            y={top}
            width={Math.max(0, x(cueOut) - x(outroStart))}
            height={bottom - top}
            fill="rgba(63,185,80,0.18)"
          />
        )}

        {ticks.map((s) => {
          const tx = x(s * 1000);
          return (
            <g key={s}>
              <line x1={tx} y1={top} x2={tx} y2={bottom} stroke="var(--border)" strokeWidth={1} opacity={0.5} />
              <text
                x={tx > width - 26 ? tx - 3 : tx + 3}
                y={height - 5}
                textAnchor={tx > width - 26 ? "end" : "start"}
                fontSize={9}
                fill="var(--text-muted)"
              >
                {tickLabel(s)}
              </text>
            </g>
          );
        })}

        {peaks.map((peak, i) => {
          const amp = (Math.max(0, Math.min(255, peak)) / 255) * amplitude;
          return (
            <rect
              key={i}
              x={i * barWidth}
              y={mid - amp}
              width={Math.max(1, barWidth * 0.9)}
              height={Math.max(1, amp * 2)}
              fill="var(--text-secondary)"
              opacity={0.75}
            />
          );
        })}

        {positionMs != null && durationMs > 0 && (
          <g>
            <line x1={x(positionMs)} y1={top} x2={x(positionMs)} y2={bottom} stroke="var(--red)" strokeWidth={1.5} />
            <polygon
              points={`${x(positionMs) - 4},${top - 2} ${x(positionMs) + 4},${top - 2} ${x(positionMs)},${top + 4}`}
              fill="var(--red)"
            />
          </g>
        )}
      </svg>

      {marks.filter((m) => m.ms != null).map((m) => {
        const left = x(m.ms as number);
        return (
          <div
            key={m.id}
            style={{ position: "absolute", left, top: 0, height, transform: "translateX(-50%)", pointerEvents: "none" }}
          >
            <div
              style={{
                position: "absolute", top: 0, bottom: RULER, width: 1, background: m.color, opacity: 0.95,
              }}
            />
            <button
              type="button"
              role="slider"
              aria-label={m.label}
              aria-valuemin={0}
              aria-valuemax={Math.round(durationMs)}
              aria-valuenow={Math.round(m.ms as number)}
              title={`${m.label} — ${tickLabel((m.ms as number) / 1000)}`}
              style={{
                pointerEvents: "auto", position: "absolute", top: -2, left: "50%", transform: "translateX(-50%)",
                width: 10, height: 10, padding: 0, borderRadius: 2, background: m.color,
                border: "1px solid rgba(0,0,0,0.6)", cursor: "ew-resize", touchAction: "none",
              }}
              onPointerDown={(e) => {
                e.preventDefault();
                e.stopPropagation();
                e.currentTarget.setPointerCapture(e.pointerId);
                drag.current = m.id;
                onMove(m.id, msAt(e.clientX));
              }}
              onPointerMove={(e) => {
                if (drag.current === m.id) onMove(m.id, msAt(e.clientX));
              }}
              onPointerUp={(e) => {
                if (drag.current === m.id) {
                  drag.current = null;
                  try { e.currentTarget.releasePointerCapture(e.pointerId); } catch { /* not captured */ }
                }
              }}
              onPointerCancel={(e) => {
                if (drag.current === m.id) {
                  drag.current = null;
                  try { e.currentTarget.releasePointerCapture(e.pointerId); } catch { /* not captured */ }
                }
              }}
              onKeyDown={(e) => {
                const dir = e.key === "ArrowLeft" ? -1 : e.key === "ArrowRight" ? 1 : 0;
                if (dir === 0) return;
                e.preventDefault();
                e.stopPropagation();
                moveBy(m.id, m.ms as number, dir * (e.shiftKey ? 1000 : 100));
              }}
            />
          </div>
        );
      })}
    </div>
  );
}
