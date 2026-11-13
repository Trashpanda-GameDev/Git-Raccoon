import type { DiffLine, FileDiff } from "../types";
import { highlightLines, languageFor } from "./highlight";
import { markRanges, wordDiff } from "./wordDiff";

/** How many added lines ahead to look for a deleted line's counterpart. */
const LOOKAHEAD = 8;

/**
 * HTML for every line of a diff: syntax highlighted, with the changed words
 * of each modified line marked. Keyed by line object.
 */
export function renderDiff(diff: FileDiff): Map<DiffLine, string> {
  const language = languageFor(diff.path);
  const html = new Map<DiffLine, string>();

  for (const hunk of diff.hunks) {
    // Highlight each side of the hunk as continuous text.
    const oldSide = hunk.lines.filter((l) => l.kind === "context" || l.kind === "delete");
    const newSide = hunk.lines.filter((l) => l.kind === "context" || l.kind === "add");
    const oldHtml = highlightLines(oldSide.map((l) => l.content), language);
    const newHtml = highlightLines(newSide.map((l) => l.content), language);
    oldSide.forEach((l, i) => l.kind === "delete" && html.set(l, oldHtml[i]));
    newSide.forEach((l, i) => html.set(l, newHtml[i]));

    // Pair each run of deletions with the additions right after it.
    for (let i = 0; i < hunk.lines.length; ) {
      if (hunk.lines[i].kind !== "delete") {
        i++;
        continue;
      }
      const dels: DiffLine[] = [];
      const adds: DiffLine[] = [];
      while (i < hunk.lines.length && hunk.lines[i].kind === "delete") dels.push(hunk.lines[i++]);
      while (i < hunk.lines.length && (hunk.lines[i].kind === "add" || hunk.lines[i].kind === "noeol")) {
        if (hunk.lines[i].kind === "add") adds.push(hunk.lines[i]);
        i++;
      }
      // Match each deleted line with the most similar added line among the
      // next few, keeping order, so inserted lines don't shift the pairing.
      let from = 0;
      for (const del of dels) {
        let best: { k: number; words: NonNullable<ReturnType<typeof wordDiff>> } | null = null;
        for (let k = from; k < Math.min(adds.length, from + LOOKAHEAD); k++) {
          const words = wordDiff(del.content, adds[k].content);
          if (words && (!best || words.similarity > best.words.similarity)) best = { k, words };
        }
        if (!best) continue;
        html.set(del, markRanges(html.get(del)!, best.words.old, "word-del"));
        html.set(adds[best.k], markRanges(html.get(adds[best.k])!, best.words.new, "word-add"));
        from = best.k + 1;
      }
    }
  }
  return html;
}
