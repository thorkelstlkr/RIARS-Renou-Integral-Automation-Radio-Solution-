#!/usr/bin/env node
// Builds the online user manual for the website from the in-app manual (src/help/<lang>/*.md).
//
//   node scripts/build-manual.mjs           write website/manual/ (Spanish) and website/en/manual/ (English)
//   node scripts/build-manual.mjs --check   exit 1 if the committed pages are out of date
//
// The website is plain static files, so the generated pages are committed next to the hand-written
// ones. No dependencies: the Markdown subset is the one src/help/Markdown.tsx renders in the app,
// plus nested lists.

import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const HELP = join(ROOT, "src", "help");
const SITE = join(ROOT, "website");
const BASE = "https://thorkelstlkr.github.io/RIARS-Renou-Integral-Automation-Radio-Solution-/";
const REPO = "https://github.com/thorkelstlkr/RIARS-Renou-Integral-Automation-Radio-Solution-";

const SECTIONS = ["start", "onair", "scheduling", "automation", "advertising", "logs", "remote", "reference"];

// Site languages: Spanish at the root, English under /en/. `dir` is relative to website/.
const LANGS = {
  es: {
    dir: "manual",
    up: "../",
    other: "en",
    locale: "es_ES",
    t: {
      manual: "Manual de usuario",
      manualShort: "Manual",
      titleSuffix: "Manual de RIARS",
      lead: "Todo lo que hace RIARS, paso a paso: el mismo manual que trae el programa en Ayuda ▸ Manual (F1).",
      home: "Portada",
      download: "Descargar",
      skip: "Saltar al contenido",
      search: "Buscar en el manual",
      noResults: "No hay resultados.",
      contents: "Índice del manual",
      toContents: "Índice",
      prev: "Anterior",
      next: "Siguiente",
      edit: "Mejorar esta página en GitHub",
      fallback: "Este tema aún no está traducido; se muestra en inglés.",
      mainNav: "Principal",
      footerNav: "Pie de página",
      footer: "RIARS — Renou Integral Automation Radio Solution. Software libre con licencia MIT.",
      issues: "Incidencias",
      license: "Licencia",
      signing: "Política de firma de código",
      privacy: "Privacidad",
      otherLang: "English",
      sections: {
        start: "Primeros pasos",
        onair: "En antena",
        scheduling: "Programación",
        automation: "Automatización",
        advertising: "Publicidad",
        logs: "Registros",
        remote: "Control remoto y servidores",
        reference: "Referencia",
      },
    },
  },
  en: {
    dir: "en/manual",
    up: "../../",
    other: "es",
    locale: "en_GB",
    t: {
      manual: "User manual",
      manualShort: "Manual",
      titleSuffix: "RIARS manual",
      lead: "Everything RIARS does, step by step: the same manual the program has under Help ▸ Manual (F1).",
      home: "Home",
      download: "Download",
      skip: "Skip to content",
      search: "Search the manual",
      noResults: "No results.",
      contents: "Manual contents",
      toContents: "Contents",
      prev: "Previous",
      next: "Next",
      edit: "Improve this page on GitHub",
      fallback: "This topic is not translated yet; it is shown in English.",
      mainNav: "Main",
      footerNav: "Footer",
      footer: "RIARS — Renou Integral Automation Radio Solution. Free software under the MIT licence.",
      issues: "Issues",
      license: "Licence",
      signing: "Code signing policy",
      privacy: "Privacy",
      otherLang: "Español",
      sections: {
        start: "Getting started",
        onair: "On air",
        scheduling: "Scheduling",
        automation: "Automation",
        advertising: "Advertising",
        logs: "Logs",
        remote: "Remote control and servers",
        reference: "Reference",
      },
    },
  },
};

// ---------------------------------------------------------------- topics

function unquote(value) {
  const v = value.trim();
  if ((v.startsWith('"') && v.endsWith('"')) || (v.startsWith("'") && v.endsWith("'"))) return v.slice(1, -1);
  return v;
}

function parseTopic(lang, file) {
  const raw = readFileSync(join(HELP, lang, file), "utf8");
  const lines = raw.replace(/^﻿/, "").replace(/\r\n?/g, "\n").split("\n");
  const meta = {};
  let bodyStart = 0;
  if (lines[0]?.trim() === "---") {
    const end = lines.findIndex((line, i) => i > 0 && line.trim() === "---");
    if (end > 0) {
      for (const line of lines.slice(1, end)) {
        const colon = line.indexOf(":");
        if (colon > 0) meta[line.slice(0, colon).trim().toLowerCase()] = unquote(line.slice(colon + 1));
      }
      bodyStart = end + 1;
    }
  }
  const id = meta.id?.trim() || file.replace(/\.md$/, "");
  const order = Number.parseInt(meta.order ?? "", 10);
  return {
    id,
    lang,
    file,
    title: meta.title?.trim() || id,
    section: meta.section?.trim() || "reference",
    order: Number.isFinite(order) ? order : 0,
    keywords: (meta.keywords ?? "").split(",").map((k) => k.trim()).filter(Boolean),
    body: lines.slice(bodyStart).join("\n").replace(/^\n+/, ""),
  };
}

function loadTopics(lang) {
  const dir = join(HELP, lang);
  if (!existsSync(dir)) return [];
  return readdirSync(dir).filter((f) => f.endsWith(".md")).sort().map((f) => parseTopic(lang, f));
}

function compareTopics(a, b) {
  const sa = SECTIONS.indexOf(a.section) < 0 ? SECTIONS.length : SECTIONS.indexOf(a.section);
  const sb = SECTIONS.indexOf(b.section) < 0 ? SECTIONS.length : SECTIONS.indexOf(b.section);
  return sa - sb || a.order - b.order || a.title.localeCompare(b.title) || a.id.localeCompare(b.id);
}

// Same rule as the app: every English topic, replaced by the translation where there is one.
function topicsFor(lang, english) {
  const own = new Map(loadTopics(lang).map((t) => [t.id, t]));
  const ids = new Set([...english.keys(), ...own.keys()]);
  return [...ids]
    .map((id) => (own.has(id) ? { ...own.get(id), fallback: false } : { ...english.get(id), fallback: lang !== "en" }))
    .sort(compareTopics);
}

// ---------------------------------------------------------------- markdown

const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

const fold = (s) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

function slugify(text) {
  const plain = text.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1").replace(/[`*_~]/g, "");
  return fold(plain).replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") || "section";
}

const indentOf = (line) => line.length - line.trimStart().length;
const LIST_ITEM = /^(\s*)([-*+]|\d+\.)(\s+)(.*)$/;
const isRule = (line) => /^\s*(?:-{3,}|\*{3,}|_{3,})\s*$/.test(line);
const isHeading = (line) => /^\s{0,3}#{1,3}\s+/.test(line);

function splitRow(line) {
  let v = line.trim();
  if (v.startsWith("|")) v = v.slice(1);
  if (v.endsWith("|")) v = v.slice(0, -1);
  const cells = [];
  let cur = "";
  for (let i = 0; i < v.length; i += 1) {
    if (v[i] === "\\" && v[i + 1] === "|") {
      cur += "|";
      i += 1;
    } else if (v[i] === "|") {
      cells.push(cur.trim());
      cur = "";
    } else cur += v[i];
  }
  cells.push(cur.trim());
  return cells;
}

const isTableSep = (line) => line.includes("|") && splitRow(line).every((c) => /^:?-{3,}:?$/.test(c.trim()));

function isBlockStart(lines, i) {
  const line = lines[i] ?? "";
  return (
    !line.trim() ||
    line.trim().startsWith("```") ||
    isHeading(line) ||
    isRule(line) ||
    /^\s*>\s?/.test(line) ||
    LIST_ITEM.test(line) ||
    (line.includes("|") && i + 1 < lines.length && isTableSep(lines[i + 1]))
  );
}

function parseBlocks(lines) {
  const blocks = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      i += 1;
      continue;
    }

    if (line.trim().startsWith("```")) {
      const code = [];
      const fence = indentOf(line);
      i += 1;
      while (i < lines.length && !lines[i].trim().startsWith("```")) {
        code.push(lines[i].slice(Math.min(fence, indentOf(lines[i]))));
        i += 1;
      }
      i += 1;
      blocks.push({ kind: "code", text: code.join("\n") });
      continue;
    }

    const heading = /^\s{0,3}(#{1,3})\s+(.*)$/.exec(line);
    if (heading) {
      blocks.push({ kind: "heading", level: heading[1].length, text: heading[2].trim() });
      i += 1;
      continue;
    }

    if (isRule(line)) {
      blocks.push({ kind: "rule" });
      i += 1;
      continue;
    }

    if (/^\s*>\s?/.test(line)) {
      const quote = [];
      while (i < lines.length && /^\s*>\s?/.test(lines[i])) {
        quote.push(lines[i].replace(/^\s*>\s?/, ""));
        i += 1;
      }
      blocks.push({ kind: "quote", lines: quote });
      continue;
    }

    if (line.includes("|") && i + 1 < lines.length && isTableSep(lines[i + 1])) {
      const header = splitRow(line);
      const align = splitRow(lines[i + 1]).map((c) => {
        const v = c.trim();
        if (v.startsWith(":") && v.endsWith(":")) return "center";
        if (v.endsWith(":")) return "right";
        return v.startsWith(":") ? "left" : "";
      });
      i += 2;
      const rows = [];
      while (i < lines.length && lines[i].trim() && lines[i].includes("|") && !isTableSep(lines[i])) {
        rows.push(splitRow(lines[i]));
        i += 1;
      }
      blocks.push({ kind: "table", header, align, rows });
      continue;
    }

    const first = LIST_ITEM.exec(line);
    if (first) {
      const base = first[1].length;
      const ordered = /\d/.test(first[2]);
      const start = ordered ? Number.parseInt(first[2], 10) : 1;
      const items = [];
      while (i < lines.length) {
        const m = LIST_ITEM.exec(lines[i]);
        if (!m || m[1].length !== base || /\d/.test(m[2]) !== ordered) break;
        const content = base + m[2].length + m[3].length;
        const body = [m[4]];
        i += 1;
        // The item goes on while lines are indented past its marker, or are lazy continuations
        // of its text; a blank line ends it unless indented content follows.
        while (i < lines.length) {
          const next = lines[i];
          if (!next.trim()) {
            let j = i;
            while (j < lines.length && !lines[j].trim()) j += 1;
            if (j < lines.length && indentOf(lines[j]) > base) {
              body.push("");
              i += 1;
              continue;
            }
            break;
          }
          if (indentOf(next) > base) {
            body.push(next.slice(Math.min(content, indentOf(next))));
            i += 1;
            continue;
          }
          if (body[body.length - 1].trim() && !isBlockStart(lines, i)) {
            body.push(next.trim());
            i += 1;
            continue;
          }
          break;
        }
        items.push(parseBlocks(body));
      }
      blocks.push({ kind: "list", ordered, start, items });
      continue;
    }

    const para = [];
    while (i < lines.length && !isBlockStart(lines, i)) {
      para.push(lines[i].trim());
      i += 1;
    }
    if (para.length) blocks.push({ kind: "paragraph", lines: para });
    else i += 1;
  }
  return blocks;
}

class Renderer {
  constructor(ids, missing) {
    this.ids = ids; // topic ids that have a page
    this.missing = missing; // collects broken topic links
  }

  inline(text) {
    const pattern = /(\*\*[^*]+\*\*|\*[^*]+\*|`[^`]+`|\[[^\]]+\]\([^)]+\))/g;
    let out = "";
    let last = 0;
    let m;
    while ((m = pattern.exec(text)) !== null) {
      out += esc(text.slice(last, m.index));
      const tok = m[0];
      if (tok.startsWith("**")) out += `<strong>${this.inline(tok.slice(2, -2))}</strong>`;
      else if (tok.startsWith("`")) out += `<code>${esc(tok.slice(1, -1))}</code>`;
      else if (tok.startsWith("*")) out += `<em>${this.inline(tok.slice(1, -1))}</em>`;
      else {
        const link = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(tok);
        const label = this.inline(link[1]);
        const target = link[2].trim();
        if (target.startsWith("topic:")) {
          const [id, anchor] = target.slice(6).trim().split("#");
          if (!this.ids.has(id)) this.missing.push(target);
          out += `<a href="${esc(id)}.html${anchor ? `#${esc(anchor)}` : ""}">${label}</a>`;
        } else if (/^https?:\/\//i.test(target)) {
          out += `<a href="${esc(target)}" target="_blank" rel="noopener">${label}</a>`;
        } else out += label;
      }
      last = m.index + tok.length;
    }
    return out + esc(text.slice(last));
  }

  blocks(blocks, { skipTitle = false } = {}) {
    return blocks
      .filter((b, i) => !(skipTitle && i === 0 && b.kind === "heading" && b.level === 1))
      .map((b) => this.block(b))
      .join("\n");
  }

  block(b) {
    switch (b.kind) {
      case "heading": {
        const tag = `h${b.level}`;
        return `<${tag} id="${slugify(b.text)}">${this.inline(b.text)}</${tag}>`;
      }
      case "paragraph":
        return `<p>${b.lines.map((l) => this.inline(l)).join("<br>\n")}</p>`;
      case "list": {
        const tag = b.ordered ? "ol" : "ul";
        const start = b.ordered && b.start !== 1 ? ` start="${b.start}"` : "";
        const items = b.items.map((children) => {
          // A first paragraph sits directly in the <li>, like a tight list.
          const [head, ...rest] = children;
          const lead = head?.kind === "paragraph" ? head.lines.map((l) => this.inline(l)).join("<br>\n") : this.block(head ?? { kind: "rule" });
          const more = (head?.kind === "paragraph" ? rest : children.slice(1)).map((c) => this.block(c)).join("\n");
          return `<li>${lead}${more ? `\n${more}` : ""}</li>`;
        });
        return `<${tag}${start}>\n${items.join("\n")}\n</${tag}>`;
      }
      case "code":
        return `<pre><code>${esc(b.text)}</code></pre>`;
      case "quote": {
        const label = /^(tip|note|warning):\s*/i.exec(b.lines[0] ?? "");
        const kind = label ? label[1].toLowerCase() : "";
        const lines = b.lines.map((l, i) => {
          if (i === 0 && label) return `<strong>${esc(label[1])}:</strong> ${this.inline(l.slice(label[0].length))}`;
          return this.inline(l);
        });
        return `<div class="callout${kind ? ` callout-${kind}` : ""}">\n${lines.map((l) => `<p>${l}</p>`).join("\n")}\n</div>`;
      }
      case "table": {
        const cell = (tag, text, i) => {
          const a = b.align[i];
          return `<${tag}${a ? ` style="text-align:${a}"` : ""}>${this.inline(text)}</${tag}>`;
        };
        const head = `<tr>${b.header.map((c, i) => cell("th", c, i)).join("")}</tr>`;
        const rows = b.rows.map((r) => `<tr>${r.map((c, i) => cell("td", c, i)).join("")}</tr>`).join("\n");
        return `<div class="table-wrap"><table>\n<thead>${head}</thead>\n<tbody>\n${rows}\n</tbody>\n</table></div>`;
      }
      case "rule":
        return "<hr>";
      default:
        return "";
    }
  }
}

function plainText(markdown) {
  return markdown
    .replace(/```[^\n]*\n?/g, " ")
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
    .replace(/\s+/g, " ")
    .trim();
}

function clip(text, max) {
  if (text.length <= max) return text;
  const cut = text.slice(0, max);
  return `${cut.slice(0, Math.max(cut.lastIndexOf(" "), max - 20)).replace(/[\s,.;:]+$/, "")}…`;
}

// First paragraph after the title, as plain text.
function summary(topic) {
  const blocks = parseBlocks(topic.body.split("\n"));
  const para = blocks.find((b) => b.kind === "paragraph");
  return para ? plainText(para.lines.join(" ")) : plainText(topic.body);
}

// ---------------------------------------------------------------- pages

function pageUrl(lang, file) {
  return `${BASE}${LANGS[lang].dir}/${file === "index.html" ? "" : file}`;
}

function head({ lang, title, description, file, otherHasPage }) {
  const L = LANGS[lang];
  const up = L.up;
  const alt = (l) => pageUrl(l, file);
  const hreflang = otherHasPage
    ? [
        `  <link rel="alternate" hreflang="es" href="${alt("es")}">`,
        `  <link rel="alternate" hreflang="en" href="${alt("en")}">`,
        `  <link rel="alternate" hreflang="x-default" href="${alt("es")}">`,
      ].join("\n")
    : "";
  return `<!doctype html>
<html lang="${lang}">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${esc(title)}</title>
  <meta name="description" content="${esc(description)}">
  <meta name="theme-color" content="#0d1117">
  <meta property="og:type" content="article">
  <meta property="og:title" content="${esc(title)}">
  <meta property="og:description" content="${esc(description)}">
  <meta property="og:url" content="${pageUrl(lang, file)}">
  <meta property="og:site_name" content="RIARS">
  <meta property="og:locale" content="${L.locale}">
  <meta property="og:image" content="${BASE}img/og-${lang}.jpg">
  <link rel="canonical" href="${pageUrl(lang, file)}">
${hreflang}
  <link rel="icon" type="image/png" href="${up}img/favicon.png">
  <link rel="apple-touch-icon" href="${up}img/apple-touch-icon.png">
  <link rel="stylesheet" href="${up}style.css">
  <script src="${up}manual.js" defer></script>
</head>`.replace(/\n\n/g, "\n");
}

function header(lang, otherHref) {
  const L = LANGS[lang];
  const O = LANGS[L.other];
  return `<body class="manual-page">
  <a class="skip" href="#main">${L.t.skip}</a>

  <header class="site-header">
    <div class="wrap">
      <a class="brand" href="${L.up}${lang === "en" ? "en/" : ""}"><img src="${L.up}img/logo-horizontal.png" alt="RIARS" width="143" height="34" loading="eager"></a>
      <nav class="site-nav" aria-label="${L.t.mainNav}">
        <a class="nav-link" href="${L.up}${lang === "en" ? "en/" : ""}">${L.t.home}</a>
        <a class="nav-link manual-link" href="./"${otherHref === "./" ? ' aria-current="page"' : ""}>${L.t.manualShort}</a>
        <a class="nav-link" href="${L.up}${lang === "en" ? "en/" : ""}#download">${L.t.download}</a>
        <a class="lang" href="${otherHref === "./" ? `${L.up}${O.dir}/` : `${L.up}${O.dir}/${otherHref}`}" hreflang="${L.other}" lang="${L.other}" aria-label="${L.t.otherLang}">${L.other.toUpperCase()}</a>
      </nav>
    </div>
  </header>`;
}

function footer(lang) {
  const L = LANGS[lang];
  const root = `${L.up}${lang === "en" ? "en/" : ""}`;
  const other = L.other === "en" ? `${L.up}en/` : L.up;
  return `  <footer class="site-footer">
    <div class="wrap">
      <div>
        <img src="${L.up}img/logo-horizontal.png" alt="RIARS" width="117" height="28" loading="lazy">
        <p>${L.t.footer}</p>
        <p>© 2026 RIARS Contributors</p>
      </div>
      <nav aria-label="${L.t.footerNav}">
        <a href="${REPO}" target="_blank" rel="noopener">GitHub</a>
        <a href="${REPO}/issues" target="_blank" rel="noopener">${L.t.issues}</a>
        <a href="${REPO}/blob/main/LICENSE" target="_blank" rel="noopener">${L.t.license}</a>
        <a href="${root}code-signing.html">${L.t.signing}</a>
        <a href="${root}privacy.html">${L.t.privacy}</a>
        <a href="${other}" hreflang="${L.other}" lang="${L.other}">${L.t.otherLang}</a>
      </nav>
    </div>
  </footer>
</body>
</html>
`;
}

function searchBox(lang) {
  const t = LANGS[lang].t;
  return `<div class="manual-search" data-no-results="${esc(t.noResults)}">
          <label class="visually-hidden" for="manual-q">${t.search}</label>
          <input id="manual-q" type="search" placeholder="${t.search}" autocomplete="off" spellcheck="false">
          <ol class="manual-results" hidden></ol>
        </div>`;
}

function grouped(topics) {
  const groups = new Map();
  for (const t of topics) {
    if (!groups.has(t.section)) groups.set(t.section, []);
    groups.get(t.section).push(t);
  }
  return groups;
}

function toc(lang, topics, current) {
  const t = LANGS[lang].t;
  const groups = [...grouped(topics)].map(([section, list]) => {
    const items = list
      .map((topic) => `<li><a href="${topic.id}.html"${topic.id === current ? ' aria-current="page"' : ""}>${esc(topic.title)}</a></li>`)
      .join("\n            ");
    return `          <h2>${esc(t.sections[section] ?? section)}</h2>
          <ul>
            ${items}
          </ul>`;
  });
  return `      <aside class="manual-toc" id="manual-toc">
        ${searchBox(lang)}
        <nav aria-label="${t.contents}">
          <p class="manual-toc-title"><a href="./">${t.manual}</a></p>
${groups.join("\n")}
        </nav>
      </aside>`;
}

function topicPage(lang, topics, index, renderer) {
  const L = LANGS[lang];
  const topic = topics[index];
  const blocks = parseBlocks(topic.body.split("\n"));
  const h1 = blocks[0]?.kind === "heading" && blocks[0].level === 1 ? blocks[0].text : topic.title;
  const description = clip(summary(topic), 155);
  const prev = topics[index - 1];
  const next = topics[index + 1];
  const pager = [
    prev ? `<a class="prev" href="${prev.id}.html" rel="prev"><span>${L.t.prev}</span>${esc(prev.title)}</a>` : "<span></span>",
    next ? `<a class="next" href="${next.id}.html" rel="next"><span>${L.t.next}</span>${esc(next.title)}</a>` : "<span></span>",
  ].join("\n          ");
  const contentLang = topic.fallback ? ' lang="en"' : "";
  const file = `${topic.id}.html`;
  return `${head({ lang, title: `${topic.title} — ${L.t.titleSuffix}`, description, file, otherHasPage: true })}
${header(lang, file)}

  <main id="main" class="manual">
    <div class="wrap manual-layout">
      <article class="manual-body"${contentLang}>
        <p class="kicker"><a href="./">${L.t.manualShort}</a> · ${esc(L.t.sections[topic.section] ?? topic.section)}<a class="to-toc" href="#manual-toc">${L.t.toContents}</a></p>
${topic.fallback ? `        <p class="callout callout-note" lang="${lang}">${L.t.fallback}</p>\n` : ""}        <h1 id="${slugify(h1)}">${renderer.inline(h1)}</h1>
${renderer.blocks(blocks, { skipTitle: true })}
        <nav class="manual-pager" aria-label="${L.t.contents}">
          ${pager}
        </nav>
        <p class="manual-edit"><a href="${REPO}/blob/main/src/help/${topic.lang}/${topic.file}" target="_blank" rel="noopener">${L.t.edit}</a></p>
      </article>
${toc(lang, topics, topic.id)}
    </div>
  </main>

${footer(lang)}`;
}

function indexPage(lang, topics) {
  const L = LANGS[lang];
  const sections = [...grouped(topics)].map(([section, list]) => {
    const items = list
      .map((t) => `            <li><a href="${t.id}.html">${esc(t.title)}</a><p>${esc(clip(summary(t), 150))}</p></li>`)
      .join("\n");
    return `        <section class="manual-section" aria-labelledby="s-${section}">
          <h2 id="s-${section}">${esc(L.t.sections[section] ?? section)}</h2>
          <ul>
${items}
          </ul>
        </section>`;
  });
  return `${head({ lang, title: `${L.t.manual} — RIARS`, description: L.t.lead, file: "index.html", otherHasPage: true })}
${header(lang, "./")}

  <main id="main" class="manual">
    <div class="wrap manual-index">
      <p class="kicker">RIARS</p>
      <h1>${L.t.manual}</h1>
      <p class="lead">${esc(L.t.lead)}</p>
      ${searchBox(lang)}
      <div class="manual-sections">
${sections.join("\n")}
      </div>
    </div>
  </main>

${footer(lang)}`;
}

function searchIndex(topics) {
  return `${JSON.stringify(
    topics.map((t) => ({ id: t.id, title: t.title, keywords: t.keywords.join(", "), text: plainText(t.body.replace(/^#\s+.*\n/, "")) })),
  )}\n`;
}

function sitemap(current, langsTopics) {
  const entry = (file) => {
    const alts = ["es", "en", "x-default"]
      .map((l) => `    <xhtml:link rel="alternate" hreflang="${l}" href="${pageUrl(l === "x-default" ? "es" : l, file)}"/>`)
      .join("\n");
    return ["es", "en"].map((l) => `  <url>\n    <loc>${pageUrl(l, file)}</loc>\n${alts}\n  </url>`).join("\n");
  };
  const files = ["index.html", ...langsTopics.es.map((t) => `${t.id}.html`)];
  const block = `  <!-- manual:start (generated by scripts/build-manual.mjs) -->\n${files.map(entry).join("\n")}\n  <!-- manual:end -->`;
  const text = current.replace(/\r\n/g, "\n");
  if (text.includes("<!-- manual:start")) return text.replace(/ {2}<!-- manual:start[\s\S]*?<!-- manual:end -->/, block);
  return text.replace("</urlset>", `${block}\n</urlset>`);
}

// ---------------------------------------------------------------- main

function build() {
  const english = new Map(loadTopics("en").map((t) => [t.id, t]));
  const out = new Map(); // path relative to website/ -> content
  const missing = [];
  const langsTopics = {};
  for (const lang of Object.keys(LANGS)) {
    const topics = topicsFor(lang, english);
    langsTopics[lang] = topics;
    const renderer = new Renderer(new Set(topics.map((t) => t.id)), missing);
    const dir = LANGS[lang].dir;
    out.set(`${dir}/index.html`, indexPage(lang, topics));
    topics.forEach((t, i) => out.set(`${dir}/${t.id}.html`, topicPage(lang, topics, i, renderer)));
    out.set(`${dir}/search.json`, searchIndex(topics));
  }
  out.set("sitemap.xml", sitemap(readFileSync(join(SITE, "sitemap.xml"), "utf8"), langsTopics));
  if (missing.length) {
    console.error(`Broken topic links: ${[...new Set(missing)].join(", ")}`);
    process.exit(1);
  }
  return out;
}

function generatedOnDisk() {
  const files = [];
  for (const { dir } of Object.values(LANGS)) {
    const abs = join(SITE, dir);
    if (!existsSync(abs)) continue;
    for (const f of readdirSync(abs)) files.push(`${dir}/${f}`);
  }
  return files;
}

const check = process.argv.includes("--check");
const out = build();
const stale = [];
for (const [rel, content] of out) {
  const abs = join(SITE, rel);
  const before = existsSync(abs) ? readFileSync(abs, "utf8").replace(/\r\n/g, "\n") : null;
  if (before === content) continue;
  stale.push(rel);
  if (!check) {
    mkdirSync(dirname(abs), { recursive: true });
    writeFileSync(abs, content);
  }
}
for (const rel of generatedOnDisk()) {
  if (out.has(rel)) continue;
  stale.push(`${rel} (no longer in the manual)`);
  if (!check) rmSync(join(SITE, rel));
}

if (check) {
  if (stale.length) {
    console.error(`The online manual is out of date. Run: npm run manual\n  ${stale.join("\n  ")}`);
    process.exit(1);
  }
  console.log("Online manual is up to date.");
} else {
  console.log(stale.length ? `Updated ${stale.length} file(s) in ${relative(ROOT, SITE)}/` : "Online manual already up to date.");
}
