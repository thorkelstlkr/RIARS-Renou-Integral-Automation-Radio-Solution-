import { useState, useEffect, useRef } from "react";

export default function VuMeter() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [levelL, setLevelL] = useState(0);
  const [levelR, setLevelR] = useState(0);

  useEffect(() => {
    let frame: number;
    const animate = () => {
      setLevelL((p) => p + (0.3 + Math.random() * 0.4 - p) * 0.15);
      setLevelR((p) => p + (0.3 + Math.random() * 0.4 - p) * 0.15);
      frame = requestAnimationFrame(animate);
    };
    frame = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(frame);
  }, []);

  useEffect(() => {
    const c = canvasRef.current;
    if (!c) return;
    const ctx = c.getContext("2d");
    if (!ctx) return;
    const w = c.width, h = c.height, bw = (w - 12) / 2;
    ctx.clearRect(0, 0, w, h);
    drawBar(ctx, 4, bw, h, levelL);
    drawBar(ctx, bw + 8, bw, h, levelR);
  }, [levelL, levelR]);

  return (
    <div style={{ padding: 12, flex: 1, display: "flex", flexDirection: "column", alignItems: "center" }}>
      <div style={{ fontSize: 11, fontWeight: 600, color: "var(--text-muted)", textTransform: "uppercase" as const, letterSpacing: 1, marginBottom: 8 }}>
        VU Meter
      </div>
      <canvas ref={canvasRef} width={200} height={140} style={{ width: "100%", maxWidth: 200, imageRendering: "pixelated" }} />
      <div style={{ display: "flex", justifyContent: "space-around", width: "100%", maxWidth: 200, marginTop: 8, fontSize: 11, fontFamily: "var(--font-mono)", color: "var(--text-secondary)" }}>
        <span>L: {Math.round(levelL * 100)}%</span>
        <span>R: {Math.round(levelR * 100)}%</span>
      </div>
    </div>
  );
}

function drawBar(ctx: CanvasRenderingContext2D, x: number, w: number, h: number, level: number) {
  const segs = 20, gap = 2, segH = (h - gap * (segs - 1)) / segs;
  for (let i = 0; i < segs; i++) {
    const y = h - (i + 1) * (segH + gap), r = i / segs;
    ctx.fillStyle = r <= level
      ? r > 0.85 ? "#f85149" : r > 0.7 ? "#d29922" : "#3fb950"
      : "#21262d";
    ctx.fillRect(x, y, w, segH);
  }
}
