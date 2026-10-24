export const SECTIONS = [
  "start",
  "onair",
  "scheduling",
  "automation",
  "advertising",
  "logs",
  "remote",
  "reference",
] as const;

export type HelpSectionId = (typeof SECTIONS)[number];

export interface HelpTopic {
  id: string;
  lang: string;
  title: string;
  section: string;
  order: number;
  keywords: string[];
  body: string;
  fallback: boolean;
}

export interface HelpSearchResult {
  topic: HelpTopic;
  score: number;
  snippet: string;
}

type Translator = (key: string) => string;

const modules = import.meta.glob("./*/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

function unquote(value: string): string {
  const trimmed = value.trim();
  if (
    (trimmed.startsWith('"') && trimmed.endsWith('"')) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'"))
  ) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

function parseTopic(path: string, raw: string): HelpTopic | null {
  const pathMatch = /(?:^|\/)([^/]+)\/([^/]+)\.md$/.exec(path);
  const pathLang = pathMatch?.[1] ?? "en";
  const pathId = pathMatch?.[2] ?? path;
  const text = raw.replace(/^\uFEFF/, "").replace(/\r\n?/g, "\n");
  const lines = text.split("\n");
  const meta: Record<string, string> = {};
  let bodyStart = 0;

  if (lines[0]?.trim() === "---") {
    const end = lines.findIndex((line, index) => index > 0 && line.trim() === "---");
    if (end > 0) {
      for (const line of lines.slice(1, end)) {
        const colon = line.indexOf(":");
        if (colon < 0) continue;
        const key = line.slice(0, colon).trim().toLowerCase();
        if (key) meta[key] = unquote(line.slice(colon + 1));
      }
      bodyStart = end + 1;
    }
  }

  const id = meta.id?.trim() || pathId;
  if (!id) return null;

  const order = Number.parseInt(meta.order ?? "", 10);
  return {
    id,
    lang: (meta.lang?.trim() || pathLang).toLowerCase(),
    title: meta.title?.trim() || id,
    section: meta.section?.trim() || "reference",
    order: Number.isFinite(order) ? order : 0,
    keywords: (meta.keywords ?? "")
      .split(",")
      .map((keyword) => keyword.trim())
      .filter(Boolean),
    body: lines.slice(bodyStart).join("\n").replace(/^\n+/, ""),
    fallback: false,
  };
}

const ALL_TOPICS: HelpTopic[] = Object.entries(modules)
  .map(([path, raw]) => parseTopic(path, raw))
  .filter((topic): topic is HelpTopic => topic !== null);

const SECTION_INDEX = new Map<string, number>(
  SECTIONS.map((id, index) => [id, index]),
);

function compareTopics(a: HelpTopic, b: HelpTopic): number {
  const sectionA = SECTION_INDEX.get(a.section) ?? SECTIONS.length;
  const sectionB = SECTION_INDEX.get(b.section) ?? SECTIONS.length;
  if (sectionA !== sectionB) return sectionA - sectionB;
  if (a.order !== b.order) return a.order - b.order;
  return a.title.localeCompare(b.title) || a.id.localeCompare(b.id);
}

function normalizeLang(lang: string): string {
  return (lang || "en").toLowerCase().split("-")[0];
}

export function topicsFor(lang: string): HelpTopic[] {
  const code = normalizeLang(lang);
  const english = new Map<string, HelpTopic>();
  const translated = new Map<string, HelpTopic>();

  for (const topic of ALL_TOPICS) {
    if (topic.lang === "en") english.set(topic.id, topic);
    if (topic.lang === code) translated.set(topic.id, topic);
  }

  const ids = new Set<string>([...english.keys(), ...translated.keys()]);
  const result: HelpTopic[] = [];

  for (const id of ids) {
    const own = translated.get(id);
    if (own) {
      result.push({ ...own, fallback: false });
    } else {
      const base = english.get(id);
      if (base) result.push({ ...base, fallback: true });
    }
  }

  return result.sort(compareTopics);
}

export function sectionTitle(sectionId: string, t: Translator): string {
  return t(`help.sections.${sectionId}`);
}

export function getTopic(lang: string, id: string): HelpTopic | undefined {
  return topicsFor(lang).find((topic) => topic.id === id);
}

function fold(value: string): string {
  return value.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase();
}

function stripMarkdown(markdown: string): string {
  return markdown
    .replace(/```[^\n]*\n?/g, " ")
    .replace(/```/g, " ")
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/^\s{0,3}#{1,6}\s+/gm, "")
    .replace(/^\s*>\s?/gm, "")
    .replace(/^\s*(?:[-*+]|\d+\.)\s+/gm, "")
    .replace(/^\s*(?:-{3,}|\*{3,}|_{3,})\s*$/gm, " ")
    .replace(/^\s*\|?[\s:|-]{3,}\|?\s*$/gm, " ")
    .replace(/\|/g, " ")
    .replace(/(\*\*|__)(.*?)\1/g, "$2")
    .replace(/(\*|_)(.*?)\1/g, "$2")
    .replace(/`([^`]*)`/g, "$1")
    .replace(/<[^>]*>/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function countOccurrences(haystack: string, needle: string): number {
  if (!needle) return 0;
  let count = 0;
  let from = 0;
  while (count < 5) {
    const index = haystack.indexOf(needle, from);
    if (index < 0) break;
    count += 1;
    from = index + needle.length;
  }
  return count;
}

function queryWords(query: string): string[] {
  return Array.from(
    new Set(fold(query).split(/[^\p{L}\p{N}]+/u).filter(Boolean)),
  );
}

function makeSnippet(body: string, words: string[]): string {
  const plain = stripMarkdown(body);
  const folded = fold(plain);
  let first = -1;

  for (const word of words) {
    const index = folded.indexOf(word);
    if (index >= 0 && (first < 0 || index < first)) first = index;
  }

  if (first < 0) {
    const clipped = plain.slice(0, 140).trim();
    return plain.length > 140 ? `${clipped}…` : clipped;
  }

  if (first > plain.length - 1) first = Math.max(0, plain.length - 1);

  const length = 140;
  let start = Math.max(0, first - 50);
  let end = Math.min(plain.length, start + length);
  if (end - start < length) start = Math.max(0, end - length);

  if (start > 0) {
    const space = plain.indexOf(" ", start);
    if (space >= 0 && space < first) start = space + 1;
  }
  if (end < plain.length) {
    const space = plain.lastIndexOf(" ", end);
    if (space > first) end = space;
  }

  let snippet = plain.slice(start, end).trim();
  if (start > 0) snippet = `…${snippet}`;
  if (end < plain.length) snippet = `${snippet}…`;
  return snippet;
}

export function searchHelp(lang: string, query: string): HelpSearchResult[] {
  const words = queryWords(query);
  if (words.length === 0) return [];

  const topics = topicsFor(lang);
  const results: Array<HelpSearchResult & { index: number }> = [];

  topics.forEach((topic, index) => {
    const title = fold(topic.title);
    const keywords = fold(topic.keywords.join(" "));
    const body = fold(stripMarkdown(topic.body));
    let score = 0;

    for (const word of words) {
      const titleHits = countOccurrences(title, word);
      const keywordHits = countOccurrences(keywords, word);
      const bodyHits = countOccurrences(body, word);
      if (titleHits === 0 && keywordHits === 0 && bodyHits === 0) return;

      if (titleHits > 0) score += 100 + Math.min(titleHits, 5) * 10;
      else if (keywordHits > 0) score += 40 + Math.min(keywordHits, 5) * 5;
      else score += 10 + Math.min(bodyHits, 5);

      if (keywordHits > 0) score += Math.min(keywordHits, 5) * 2;
      if (bodyHits > 0) score += Math.min(bodyHits, 5);
    }

    results.push({
      topic,
      score,
      snippet: makeSnippet(topic.body, words),
      index,
    });
  });

  results.sort((a, b) => b.score - a.score || a.index - b.index);
  return results.map(({ topic, score, snippet }) => ({ topic, score, snippet }));
}
