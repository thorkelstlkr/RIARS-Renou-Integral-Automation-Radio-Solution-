import { useTranslation } from "react-i18next";
import type { Anchor, Category, Clock, ClockSlot, SlotItem } from "./api";

const SIZE = 220;
const CENTER = SIZE / 2;
const RADIUS = 84;
const LABEL_RADIUS = 96;

const point = (deg: number, radius: number) => {
  const rad = ((deg - 90) * Math.PI) / 180;
  return { x: CENTER + radius * Math.cos(rad), y: CENTER + radius * Math.sin(rad) };
};

const arcPath = (from: number, to: number, radius: number) => {
  const a = point(from, radius);
  const b = point(to, radius);
  const large = to - from > 180 ? 1 : 0;
  return `M ${a.x.toFixed(2)} ${a.y.toFixed(2)} A ${radius} ${radius} 0 ${large} 1 ${b.x.toFixed(2)} ${b.y.toFixed(2)}`;
};

// Keep the text the right way up on the lower half of the circle.
const upright = (deg: number) => {
  const norm = ((deg % 360) + 360) % 360;
  return norm > 90 && norm < 270 ? deg + 180 : deg;
};

const short = (text: string, max = 10) => (text.length > max ? `${text.slice(0, max - 1)}…` : text);

export default function ClockPie({ clock, categories }: { clock: Clock; categories: Category[] }) {
  const { t } = useTranslation();
  const category = (id: number | null) => categories.find((c) => c.id === id);
  const categoryName = (id: number | null) => category(id)?.name ?? "—";
  const categoryColor = (id: number | null) => category(id)?.color ?? "var(--text-secondary)";

  const labelOf = (item: SlotItem): string => {
    if ("Category" in item) return categoryName(item.Category.category_id);
    if ("Imaging" in item) return categoryName(item.Imaging.category_id);
    if ("Fill" in item) return categoryName(item.Fill.category_id);
    if ("AdBreak" in item) return item.AdBreak.name || t("automation.clocks.ads");
    if ("Playlist" in item) return t("automation.clocks.playlist");
    return t("automation.clocks.track");
  };

  const colorOf = (item: SlotItem): string => {
    if ("AdBreak" in item) return "var(--red)";
    if ("Imaging" in item) return "var(--yellow)";
    if ("Category" in item) return categoryColor(item.Category.category_id);
    if ("Fill" in item) return categoryColor(item.Fill.category_id);
    if ("Playlist" in item) return "var(--blue)";
    return "var(--green)";
  };

  const totalMinutes = clock.duration_minutes > 0 ? clock.duration_minutes : 60;
  const totalSeconds = totalMinutes * 60;
  const angleOf = (anchor: Anchor): number =>
    Math.min(359.9, ((anchor.minute * 60 + anchor.second) / totalSeconds) * 360);

  const anchors: { slot: ClockSlot; index: number; at: Anchor }[] = [];
  clock.slots.forEach((slot, index) => {
    if (slot.at) anchors.push({ slot, index, at: slot.at });
  });

  const groups: { start: number; end: number; items: SlotItem[] }[] = [];
  if (anchors.length === 0) {
    groups.push({ start: 0, end: 360, items: clock.slots.map((slot) => slot.item) });
  } else {
    for (let k = 0; k < anchors.length; k++) {
      const current = anchors[k];
      const next = anchors[k + 1];
      const start = angleOf(current.at);
      let end = next ? angleOf(next.at) : angleOf(anchors[0].at) + 360;
      while (end <= start) end += 360;

      const nextIndex = next ? next.index : anchors[0].index;
      const items: SlotItem[] = [];
      const count = clock.slots.length;
      let i = (current.index + 1) % count;
      let steps = 0;
      while (i !== nextIndex && steps < count) {
        if (!clock.slots[i].at) items.push(clock.slots[i].item);
        i = (i + 1) % count;
        steps++;
      }
      groups.push({ start, end, items });
    }
  }

  const tickCount = Math.min(Math.max(totalMinutes, 1), 60);

  return (
    <svg width={SIZE} height={SIZE} viewBox={`0 0 ${SIZE} ${SIZE}`} role="img" aria-label={clock.name}>
      <title>{clock.name}</title>
      <circle cx={CENTER} cy={CENTER} r={RADIUS} fill="var(--bg-secondary)" stroke="var(--border)" />

      {Array.from({ length: tickCount }, (_, i) => {
        const deg = (i / tickCount) * 360;
        const major = i % 5 === 0;
        const a = point(deg, RADIUS - 1);
        const b = point(deg, RADIUS - (major ? 7 : 4));
        return (
          <line
            key={`tick-${i}`}
            x1={a.x}
            y1={a.y}
            x2={b.x}
            y2={b.y}
            stroke={major ? "var(--text-muted)" : "var(--border)"}
            strokeWidth={major ? 1.2 : 0.7}
          />
        );
      })}

      {groups.map((group, gi) => {
        const span = group.end - group.start;
        const count = group.items.length;
        return group.items.map((item, gi2) => {
          const center = group.start + (span * (gi2 + 0.5)) / Math.max(1, count);
          const chip = Math.min((span / Math.max(1, count)) * 0.72, 14);
          const color = colorOf(item);
          const p = point(center, RADIUS * 0.44);
          const rotation = upright(center);
          return (
            <g key={`g-${gi}-${gi2}`}>
              <path
                d={arcPath(center - chip / 2, center + chip / 2, RADIUS * 0.68)}
                fill="none"
                stroke={color}
                strokeWidth={9}
                strokeLinecap="round"
                opacity={0.9}
              >
                <title>{labelOf(item)}</title>
              </path>
              <text
                x={p.x}
                y={p.y}
                transform={`rotate(${rotation} ${p.x} ${p.y})`}
                textAnchor="middle"
                dominantBaseline="middle"
                fontSize={7.5}
                fill="var(--text-secondary)"
              >
                {short(labelOf(item))}
              </text>
            </g>
          );
        });
      })}

      {anchors.map((anchor, k) => {
        const deg = angleOf(anchor.at);
        const inner = point(deg, RADIUS * 0.34);
        const outer = point(deg, RADIUS - 1);
        const dot = point(deg, RADIUS);
        const label = point(deg, LABEL_RADIUS);
        const rotation = upright(deg);
        return (
          <g key={`anchor-${k}`}>
            <line
              x1={inner.x}
              y1={inner.y}
              x2={outer.x}
              y2={outer.y}
              stroke="var(--text-primary)"
              strokeWidth={1.2}
            />
            <circle cx={dot.x} cy={dot.y} r={2.6} fill="var(--text-primary)" />
            <text
              x={label.x}
              y={label.y}
              transform={`rotate(${rotation} ${label.x} ${label.y})`}
              textAnchor="middle"
              dominantBaseline="middle"
              fontSize={8.5}
              fontWeight={600}
              fill="var(--text-primary)"
            >
              {short(labelOf(anchor.slot.item), 12)}
            </text>
          </g>
        );
      })}
    </svg>
  );
}
