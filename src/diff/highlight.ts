import hljs from "highlight.js/lib/common";

/** Skip highlighting very large diffs; plain text stays fast. */
const MAX_CHARS = 400_000;

const SPECIAL_NAMES: Record<string, string> = {
  makefile: "makefile",
  "cmakelists.txt": "cmake",
  dockerfile: "bash",
};

/** The highlight.js language for a file path, or null for plain text. */
export function languageFor(path: string): string | null {
  const name = path.split("/").pop()!.toLowerCase();
  if (SPECIAL_NAMES[name]) return hljs.getLanguage(SPECIAL_NAMES[name]) ? SPECIAL_NAMES[name] : null;
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return null;
  const ext = name.slice(dot + 1);
  return hljs.getLanguage(ext) ? ext : null;
}

export function escapeHtml(text: string) {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

const TOKEN = /(<span[^>]*>|<\/span>|\n)/;

/**
 * Splits highlighted HTML into one string per line, closing the spans open
 * at each line break and reopening them on the next line, so every line is
 * balanced HTML on its own.
 */
export function splitHtmlLines(html: string): string[] {
  const lines: string[] = [];
  const open: string[] = [];
  let line = "";
  for (const part of html.split(TOKEN)) {
    if (part === "") continue;
    if (part === "\n") {
      lines.push(line + "</span>".repeat(open.length));
      line = open.join("");
    } else if (part.startsWith("<span")) {
      open.push(part);
      line += part;
    } else if (part === "</span>") {
      open.pop();
      line += part;
    } else {
      line += part;
    }
  }
  lines.push(line + "</span>".repeat(open.length));
  return lines;
}

/**
 * Highlights consecutive lines of one file as a block (so constructs that
 * span lines are understood) and returns HTML for each line.
 */
export function highlightLines(lines: string[], language: string | null): string[] {
  const text = lines.join("\n");
  if (!language || text.length > MAX_CHARS) return lines.map(escapeHtml);
  try {
    const html = hljs.highlight(text, { language, ignoreIllegals: true }).value;
    const out = splitHtmlLines(html);
    return out.length === lines.length ? out : lines.map(escapeHtml);
  } catch {
    return lines.map(escapeHtml);
  }
}
