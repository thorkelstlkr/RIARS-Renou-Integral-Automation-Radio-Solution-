// Pure grid resolution, a direct port of crates/riars-core/src/automation/grid.rs.
// Times are "HH:MM:SS" strings and dates "YYYY-MM-DD"; everything is local time.
import type { GridRule } from "./api";

const pad2 = (n: number) => n.toString().padStart(2, "0");

export const dateKey = (d: Date) => `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;

const timeSeconds = (t: string): number => {
  const [h = 0, m = 0, s = 0] = t.split(":").map(Number);
  return h * 3600 + m * 60 + s;
};

const dateUtc = (date: string): number => {
  const [y = 0, m = 1, d = 1] = date.split("-").map(Number);
  return Date.UTC(y, m - 1, d);
};

const dateDiffDays = (from: string, to: string): number =>
  Math.round((dateUtc(to) - dateUtc(from)) / 86_400_000);

const popcount = (n: number): number => {
  let c = 0;
  while (n) {
    c += n & 1;
    n >>>= 1;
  }
  return c;
};

/**
 * The day a daily span belongs to, if `at` falls inside it.
 * `end == start` is all day; `end < start` crosses midnight and belongs to the
 * day it started on.
 */
export function spanDay(start: string, end: string, at: Date): Date | null {
  const s = timeSeconds(start);
  const e = timeSeconds(end);
  const t = at.getHours() * 3600 + at.getMinutes() * 60 + at.getSeconds();
  const day = new Date(at.getFullYear(), at.getMonth(), at.getDate());
  if (s === e) return day;
  if (s < e) return s <= t && t < e ? day : null;
  if (t >= s) return day;
  if (t < e) {
    day.setDate(day.getDate() - 1);
    return day;
  }
  return null;
}

const daysInMonth = (d: Date): number => {
  const next = new Date(d.getFullYear(), d.getMonth() + 1, 1);
  next.setDate(0);
  return next.getDate();
};

const dayMatches = (rule: GridRule, day: Date): boolean => {
  const weekday = (day.getDay() + 6) % 7; // Monday = 0
  const week = Math.floor((day.getDate() - 1) / 7);
  const lastWeek = day.getDate() + 7 > daysInMonth(day);
  const key = dateKey(day);
  return (
    (rule.weekdays & (1 << weekday)) !== 0 &&
    (rule.months & (1 << day.getMonth())) !== 0 &&
    ((rule.weeks_of_month & (1 << week)) !== 0 ||
      (lastWeek && (rule.weeks_of_month & (1 << 5)) !== 0)) &&
    (rule.date_from == null || key >= rule.date_from) &&
    (rule.date_to == null || key <= rule.date_to)
  );
};

export function ruleMatches(rule: GridRule, at: Date): boolean {
  if (!rule.enabled) return false;
  const day = spanDay(rule.start, rule.end, at);
  return day != null && dayMatches(rule, day);
}

/**
 * Roughly how much time a rule covers in a year. Among rules of equal priority
 * the one covering less is the more specific.
 */
export function coverage(rule: GridRule): number {
  const s = timeSeconds(rule.start);
  const e = timeSeconds(rule.end);
  // Minutes, like grid.rs (s and e are seconds).
  const span = s === e ? 24 * 60 : ((((e - s) / 60) % (24 * 60)) + 24 * 60) % (24 * 60);
  const weeks =
    popcount(rule.weeks_of_month & 0x1f) + ((rule.weeks_of_month >> 5) & 1);
  let days =
    (365 * popcount(rule.weekdays & 0x7f)) / 7 *
    (popcount(rule.months & 0xfff) / 12) *
    Math.min(weeks / 5, 1);
  if (rule.date_from != null && rule.date_to != null) {
    days = Math.min(days, Math.max(0, dateDiffDays(rule.date_from, rule.date_to) + 1));
  }
  return span * days;
}

/** The rule that applies at `at`: highest priority, then the most specific. */
export function resolveGrid(rules: GridRule[], at: Date): GridRule | null {
  let best: GridRule | null = null;
  for (const rule of rules) {
    if (!ruleMatches(rule, at)) continue;
    if (
      best == null ||
      rule.priority > best.priority ||
      // Ties go to the later rule, as Rust's max_by does.
      (rule.priority === best.priority && coverage(rule) <= coverage(best))
    ) {
      best = rule;
    }
  }
  return best;
}
