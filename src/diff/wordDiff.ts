/** A changed span of characters, as [start, end) offsets into a line. */
export type Range = [number, number];

/** Skip word diffs whose token grid would exceed this (O(n·m) work). */
const MAX_CELLS = 200_000;
/** Lines sharing less than this fraction of their text are just "changed". */
const MIN_SIMILARITY = 0.35;

const TOKEN = /\w+|\s+|[^\w\s]/gu;

export function tokenize(line: string): string[] {
  return line.match(TOKEN) ?? [];
}

function toRanges(tokens: string[], kept: boolean[]): Range[] {
  const ranges: Range[] = [];
  let offset = 0;
  tokens.forEach((t, i) => {
    const end = offset + t.length;
    if (!kept[i]) {
      const last = ranges[ranges.length - 1];
      if (last && last[1] === offset) last[1] = end;
      else ranges.push([offset, end]);
    }
    offset = end;
  });
  return ranges;
}

/**
 * Finds the words that differ between a deleted line and the added line
 * that replaced it, via the longest common subsequence of their tokens.
 * Returns null when the lines are too different (or too long) for word
 * marks to help.
 */
export function wordDiff(oldLine: string, newLine: string): { old: Range[]; new: Range[]; similarity: number } | null {
  const a = tokenize(oldLine);
  const b = tokenize(newLine);
  if (a.length === 0 || b.length === 0 || a.length * b.length > MAX_CELLS) return null;

  // lcs[i][j] = LCS length of a[i..] and b[j..]
  const lcs = Array.from({ length: a.length + 1 }, () => new Uint32Array(b.length + 1));
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }
  const keptA = new Array<boolean>(a.length).fill(false);
  const keptB = new Array<boolean>(b.length).fill(false);
  let common = 0;
  for (let i = 0, j = 0; i < a.length && j < b.length; ) {
    if (a[i] === b[j]) {
      keptA[i++] = keptB[j++] = true;
      common += a[i - 1].length;
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) i++;
    else j++;
  }
  // Whitespace-only agreement does not make lines similar.
  const commonText = a.filter((t, i) => keptA[i] && /\S/.test(t)).join("").length;
  const similarity = commonText / Math.max(oldLine.trim().length, newLine.trim().length, 1);
  if (similarity < MIN_SIMILARITY || common === 0) return null;
  return { old: toRanges(a, keptA), new: toRanges(b, keptB), similarity };
}

/**
 * Wraps the characters of `html` (balanced HTML for one line, as made by
 * the highlighter) that fall inside `ranges` in `<mark class=...>`. Offsets
 * count characters of the line's text, so an entity like `&lt;` is one.
 */
export function markRanges(html: string, ranges: Range[], className: string): string {
  if (ranges.length === 0) return html;
  let out = "";
  let offset = 0;
  let r = 0;
  let marking = false;
  const open = () => {
    if (!marking) {
      out += `<mark class="${className}">`;
      marking = true;
    }
  };
  const close = () => {
    if (marking) {
      out += "</mark>";
      marking = false;
    }
  };
  for (let i = 0; i < html.length; ) {
    if (html[i] === "<") {
      // Never let a mark straddle a tag: close it, copy the tag, reopen.
      const end = html.indexOf(">", i) + 1;
      const wasMarking = marking;
      close();
      out += html.slice(i, end);
      i = end;
      if (wasMarking && r < ranges.length && offset < ranges[r][1]) open();
      continue;
    }
    while (r < ranges.length && offset >= ranges[r][1]) r++;
    const inRange = r < ranges.length && offset >= ranges[r][0];
    if (inRange) open();
    else close();
    const char = html[i] === "&" ? html.slice(i, html.indexOf(";", i) + 1) : html[i];
    out += char;
    i += char.length;
    offset++;
  }
  close();
  return out;
}
