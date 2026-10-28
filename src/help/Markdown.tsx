import { Fragment, type ReactNode } from "react";

interface MarkdownProps {
  source: string;
  onLink: (topicId: string) => void;
}

const S: Record<string, React.CSSProperties> = {
  root: { color: "var(--text-primary)", fontSize: 14, lineHeight: 1.6, overflowWrap: "anywhere" },
  h1: { fontSize: 26, lineHeight: 1.3, margin: "8px 0 14px", color: "var(--accent)", fontWeight: 700, letterSpacing: -0.3 },
  h2: { fontSize: 20, lineHeight: 1.35, margin: "28px 0 10px", color: "var(--accent-hover)", fontWeight: 700 },
  h3: { fontSize: 16, lineHeight: 1.4, margin: "22px 0 8px", color: "var(--text-primary)", fontWeight: 700 },
  p: { margin: "0 0 14px" },
  list: { margin: "0 0 14px", paddingLeft: 22 },
  listItem: { margin: "2px 0" },
  inlineCode: {
    fontFamily: "var(--font-mono)",
    fontSize: 12.5,
    background: "var(--bg-input)",
    border: "1px solid var(--border)",
    borderRadius: 4,
    padding: "1px 4px",
  },
  pre: {
    margin: "0 0 16px",
    padding: "12px 14px",
    background: "var(--bg-secondary)",
    border: "1px solid var(--border)",
    borderRadius: "var(--radius)",
    overflowX: "auto",
    fontFamily: "var(--font-mono)",
    fontSize: 12.5,
    lineHeight: 1.5,
  },
  code: { fontFamily: "var(--font-mono)", whiteSpace: "pre" },
  quote: {
    margin: "0 0 16px",
    padding: "10px 14px",
    borderLeft: "3px solid var(--border)",
    borderRadius: "0 var(--radius) var(--radius) 0",
    background: "var(--bg-secondary)",
  },
  quoteLine: { marginTop: 4 },
  tableWrap: { margin: "0 0 16px", overflowX: "auto" },
  table: { borderCollapse: "collapse", width: "100%", fontSize: 13 },
  th: { padding: "6px 10px", border: "1px solid var(--border)", background: "var(--bg-secondary)", textAlign: "left", fontWeight: 600 },
  td: { padding: "6px 10px", border: "1px solid var(--border)", verticalAlign: "top" },
  hr: { border: 0, borderTop: "1px solid var(--border)", margin: "24px 0" },
  link: { color: "var(--accent)", textDecoration: "underline", cursor: "pointer" },
  url: { fontFamily: "var(--font-mono)", fontSize: 12, color: "var(--text-secondary)", overflowWrap: "anywhere" },
};

type Align = "left" | "center" | "right" | undefined;

type Block =
  | { kind: "heading"; level: 1 | 2 | 3; text: string }
  | { kind: "paragraph"; lines: string[] }
  | { kind: "list"; ordered: boolean; items: string[] }
  | { kind: "code"; text: string }
  | { kind: "quote"; lines: string[] }
  | { kind: "table"; header: string[]; align: Align[]; rows: string[][] }
  | { kind: "rule" };

function splitTableRow(line: string): string[] {
  let value = line.trim();
  if (value.startsWith("|")) value = value.slice(1);
  if (value.endsWith("|")) value = value.slice(0, -1);

  const cells: string[] = [];
  let current = "";
  for (let index = 0; index < value.length; index += 1) {
    const char = value[index];
    if (char === "\\" && value[index + 1] === "|") {
      current += "|";
      index += 1;
    } else if (char === "|") {
      cells.push(current.trim());
      current = "";
    } else {
      current += char;
    }
  }
  cells.push(current.trim());
  return cells;
}

function isTableSeparator(line: string): boolean {
  if (!line.includes("|")) return false;
  const cells = splitTableRow(line);
  return cells.length > 0 && cells.every((cell) => /^:?-{3,}:?$/.test(cell.trim()));
}

function tableAlign(separator: string): Align[] {
  return splitTableRow(separator).map((cell) => {
    const value = cell.trim();
    const left = value.startsWith(":");
    const right = value.endsWith(":");
    if (left && right) return "center";
    if (right) return "right";
    if (left) return "left";
    return undefined;
  });
}

function listMatch(line: string): { ordered: boolean; text: string } | null {
  const match = /^\s*([-*+]|\d+\.)\s+(.*)$/.exec(line);
  if (!match) return null;
  return { ordered: /^\d+\.$/.test(match[1]), text: match[2] };
}

function isBlockStart(lines: string[], index: number): boolean {
  const line = lines[index] ?? "";
  if (!line.trim()) return true;
  if (line.trim().startsWith("```")) return true;
  if (/^\s{0,3}#{1,3}\s+/.test(line)) return true;
  if (/^\s*(?:-{3,}|\*{3,}|_{3,})\s*$/.test(line)) return true;
  if (/^\s*>\s?/.test(line)) return true;
  if (listMatch(line)) return true;
  if (line.includes("|") && index + 1 < lines.length && isTableSeparator(lines[index + 1])) return true;
  return false;
}

function parseBlocks(source: string): Block[] {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index];
    if (!line.trim()) {
      index += 1;
      continue;
    }

    if (line.trim().startsWith("```")) {
      index += 1;
      const code: string[] = [];
      while (index < lines.length && !lines[index].trim().startsWith("```")) {
        code.push(lines[index]);
        index += 1;
      }
      if (index < lines.length) index += 1;
      blocks.push({ kind: "code", text: code.join("\n") });
      continue;
    }

    const heading = /^\s{0,3}(#{1,3})\s+(.*)$/.exec(line);
    if (heading) {
      blocks.push({
        kind: "heading",
        level: heading[1].length as 1 | 2 | 3,
        text: heading[2].trim(),
      });
      index += 1;
      continue;
    }

    if (/^\s*(?:-{3,}|\*{3,}|_{3,})\s*$/.test(line)) {
      blocks.push({ kind: "rule" });
      index += 1;
      continue;
    }

    if (/^\s*>\s?/.test(line)) {
      const quote: string[] = [];
      while (index < lines.length && /^\s*>\s?/.test(lines[index])) {
        quote.push(lines[index].replace(/^\s*>\s?/, ""));
        index += 1;
      }
      blocks.push({ kind: "quote", lines: quote });
      continue;
    }

    if (line.includes("|") && index + 1 < lines.length && isTableSeparator(lines[index + 1])) {
      const header = splitTableRow(line);
      const align = tableAlign(lines[index + 1]);
      index += 2;
      const rows: string[][] = [];
      while (
        index < lines.length &&
        lines[index].trim() &&
        lines[index].includes("|") &&
        !isTableSeparator(lines[index])
      ) {
        rows.push(splitTableRow(lines[index]));
        index += 1;
      }
      blocks.push({ kind: "table", header, align, rows });
      continue;
    }

    const item = listMatch(line);
    if (item) {
      const ordered = item.ordered;
      const items: string[] = [];
      while (index < lines.length) {
        const next = listMatch(lines[index]);
        if (!next || next.ordered !== ordered) break;
        items.push(next.text);
        index += 1;
      }
      blocks.push({ kind: "list", ordered, items });
      continue;
    }

    const paragraph: string[] = [];
    while (index < lines.length && !isBlockStart(lines, index)) {
      paragraph.push(lines[index].trim());
      index += 1;
    }
    if (paragraph.length > 0) blocks.push({ kind: "paragraph", lines: paragraph });
    else index += 1;
  }

  return blocks;
}

function foldText(value: string): string {
  return value.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase();
}

function slugify(text: string): string {
  const plain = text
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/[`*_~]/g, "");
  return foldText(plain).replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") || "section";
}

function renderInline(
  text: string,
  onLink: (topicId: string) => void,
  keyPrefix: string,
): ReactNode[] {
  const nodes: ReactNode[] = [];
  const pattern = /(\*\*[^*]+\*\*|\*[^*]+\*|`[^`]+`|\[[^\]]+\]\([^)]+\))/g;
  let lastIndex = 0;
  let tokenIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(text)) !== null) {
    if (match.index > lastIndex) nodes.push(text.slice(lastIndex, match.index));
    const token = match[0];
    const key = `${keyPrefix}-${tokenIndex}`;
    tokenIndex += 1;

    if (token.startsWith("**") && token.endsWith("**")) {
      nodes.push(
        <strong key={key}>
          {renderInline(token.slice(2, -2), onLink, key)}
        </strong>,
      );
    } else if (token.startsWith("`") && token.endsWith("`")) {
      nodes.push(
        <code key={key} style={S.inlineCode}>
          {token.slice(1, -1)}
        </code>,
      );
    } else if (token.startsWith("*") && token.endsWith("*")) {
      nodes.push(
        <em key={key}>{renderInline(token.slice(1, -1), onLink, key)}</em>,
      );
    } else {
      const link = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(token);
      if (!link) {
        nodes.push(token);
      } else {
        const label = link[1];
        const target = link[2].trim();
        if (target.startsWith("topic:")) {
          const topicId = target.slice("topic:".length).trim();
          nodes.push(
            <a
              key={key}
              href={`#topic-${topicId}`}
              style={S.link}
              onClick={(event) => {
                event.preventDefault();
                onLink(topicId);
              }}
            >
              {renderInline(label, onLink, key)}
            </a>,
          );
        } else if (/^https?:\/\//i.test(target)) {
          if (label.trim() === target) {
            nodes.push(
              <span key={key} style={S.url}>
                {target}
              </span>,
            );
          } else {
            nodes.push(
              <span key={key}>
                {label}{" "}
                <span style={S.url}>{target}</span>
              </span>,
            );
          }
        } else {
          nodes.push(label);
        }
      }
    }

    lastIndex = match.index + token.length;
  }

  if (lastIndex < text.length) nodes.push(text.slice(lastIndex));
  return nodes;
}

function renderBlock(
  block: Block,
  index: number,
  onLink: (topicId: string) => void,
): ReactNode {
  const key = `block-${index}`;

  switch (block.kind) {
    case "heading": {
      const id = slugify(block.text);
      const style = block.level === 1 ? S.h1 : block.level === 2 ? S.h2 : S.h3;
      const content = renderInline(block.text, onLink, key);
      if (block.level === 1) {
        return <h1 key={key} id={id} style={style}>{content}</h1>;
      }
      if (block.level === 2) {
        return <h2 key={key} id={id} style={style}>{content}</h2>;
      }
      return <h3 key={key} id={id} style={style}>{content}</h3>;
    }

    case "paragraph":
      return (
        <p key={key} style={S.p}>
          {block.lines.map((line, lineIndex) => (
            <Fragment key={`${key}-line-${lineIndex}`}>
              {lineIndex > 0 ? <br /> : null}
              {renderInline(line, onLink, `${key}-line-${lineIndex}`)}
            </Fragment>
          ))}
        </p>
      );

    case "list": {
      const items = block.items.map((item, itemIndex) => (
        <li key={`${key}-item-${itemIndex}`} style={S.listItem}>
          {renderInline(item, onLink, `${key}-item-${itemIndex}`)}
        </li>
      ));
      return block.ordered ? (
        <ol key={key} style={S.list}>{items}</ol>
      ) : (
        <ul key={key} style={S.list}>{items}</ul>
      );
    }

    case "code":
      return (
        <pre key={key} style={S.pre}>
          <code style={S.code}>{block.text}</code>
        </pre>
      );

    case "quote": {
      const first = block.lines[0] ?? "";
      const label = /^(tip|note|warning):\s*/i.exec(first);
      const kind = label ? label[1].toLowerCase() : "";
      const color =
        kind === "tip"
          ? "var(--blue)"
          : kind === "warning"
            ? "var(--yellow)"
            : kind === "note"
              ? "var(--text-secondary)"
              : "var(--border)";
      const background =
        kind === "tip"
          ? "rgba(88, 166, 255, 0.10)"
          : kind === "warning"
            ? "rgba(210, 153, 34, 0.12)"
            : "var(--bg-secondary)";

      return (
        <div key={key} style={{ ...S.quote, borderLeftColor: color, background }}>
          {block.lines.map((line, lineIndex) => {
            const content =
              lineIndex === 0 && label ? (
                <>
                  <strong style={{ color }}>{label[1]}:</strong>{" "}
                  {renderInline(
                    line.slice(label[0].length),
                    onLink,
                    `${key}-quote-${lineIndex}`,
                  )}
                </>
              ) : (
                renderInline(line, onLink, `${key}-quote-${lineIndex}`)
              );
            return (
              <div
                key={`${key}-quote-${lineIndex}`}
                style={lineIndex > 0 ? S.quoteLine : undefined}
              >
                {content}
              </div>
            );
          })}
        </div>
      );
    }

    case "table":
      return (
        <div key={key} style={S.tableWrap}>
          <table style={S.table}>
            <thead>
              <tr>
                {block.header.map((cell, cellIndex) => (
                  <th
                    key={`${key}-h-${cellIndex}`}
                    style={{ ...S.th, textAlign: block.align[cellIndex] ?? "left" }}
                  >
                    {renderInline(cell, onLink, `${key}-h-${cellIndex}`)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {block.rows.map((row, rowIndex) => (
                <tr key={`${key}-r-${rowIndex}`}>
                  {row.map((cell, cellIndex) => (
                    <td
                      key={`${key}-r-${rowIndex}-${cellIndex}`}
                      style={{ ...S.td, textAlign: block.align[cellIndex] ?? "left" }}
                    >
                      {renderInline(cell, onLink, `${key}-r-${rowIndex}-${cellIndex}`)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );

    case "rule":
      return <hr key={key} style={S.hr} />;
  }
}

export default function Markdown({ source, onLink }: MarkdownProps) {
  const blocks = parseBlocks(source);
  return <div style={S.root}>{blocks.map((block, index) => renderBlock(block, index, onLink))}</div>;
}
