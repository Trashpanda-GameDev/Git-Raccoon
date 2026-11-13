import { describe, expect, it } from "vitest";
import { escapeHtml, highlightLines, languageFor, splitHtmlLines } from "./highlight";
import { markRanges, tokenize, wordDiff } from "./wordDiff";
import { renderDiff } from "./render";
import type { FileDiff } from "../types";

const text = (html: string) =>
  html.replace(/<[^>]+>/g, "").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&amp;/g, "&");
const marked = (html: string) => [...html.matchAll(/<mark[^>]*>(.*?)<\/mark>/g)].map((m) => text(m[1]));

describe("languageFor", () => {
  it("maps extensions and special names", () => {
    expect(languageFor("src/main.rs")).toBe("rs");
    expect(languageFor("a/b/App.TSX")).toBe("tsx");
    expect(languageFor("Makefile")).toBe("makefile");
    expect(languageFor("LICENSE")).toBeNull();
    expect(languageFor(".gitignore")).toBeNull();
    expect(languageFor("data.unknownext")).toBeNull();
  });
});

describe("splitHtmlLines", () => {
  it("closes and reopens spans that cross line breaks", () => {
    const lines = splitHtmlLines('a<span class="c">/* one\ntwo */</span>b');
    expect(lines).toEqual(['a<span class="c">/* one</span>', '<span class="c">two */</span>b']);
  });

  it("handles nested spans and empty lines", () => {
    const lines = splitHtmlLines('<span class="x"><span class="y">p\n\nq</span></span>');
    expect(lines).toHaveLength(3);
    expect(lines[1]).toBe('<span class="x"><span class="y"></span></span>');
    expect(lines.map(text)).toEqual(["p", "", "q"]);
  });
});

describe("highlightLines", () => {
  it("keeps one balanced line per input line, with the same text", () => {
    const src = ['let s = "multi', 'line"; // done', "/* a", "b */ fn x() {}"];
    const out = highlightLines(src, "rs");
    expect(out).toHaveLength(4);
    expect(out.map(text)).toEqual(src);
    for (const line of out) {
      expect((line.match(/<span/g) ?? []).length).toBe((line.match(/<\/span>/g) ?? []).length);
    }
    expect(out[3]).toContain("hljs-comment"); // comment continued from the line before
  });

  it("escapes plain text", () => {
    expect(highlightLines(["<a & b>"], null)).toEqual(["&lt;a &amp; b&gt;"]);
  });
});

describe("wordDiff", () => {
  it("marks only the words that changed", () => {
    const d = wordDiff("let total = sum(a, b);", "let total = sum(a, b, c);")!;
    expect(d.old).toEqual([]);
    expect(d.new.map(([s, e]) => "let total = sum(a, b, c);".slice(s, e))).toEqual([", c"]);
  });

  it("marks replaced words on both sides", () => {
    const d = wordDiff("return Err(Error);", "return Err(LoadError::Bare);")!;
    expect(d.old.map(([s, e]) => "return Err(Error);".slice(s, e))).toEqual(["Error"]);
    expect(d.new.map(([s, e]) => "return Err(LoadError::Bare);".slice(s, e))).toEqual(["LoadError::Bare"]);
  });

  it("gives up on unrelated lines", () => {
    expect(wordDiff("fn main() {}", "# Heading text here")).toBeNull();
    expect(wordDiff("", "x")).toBeNull();
  });

  it("tokenizes words, spaces and punctuation separately", () => {
    expect(tokenize("a_b  (c)")).toEqual(["a_b", "  ", "(", "c", ")"]);
  });
});

describe("markRanges", () => {
  it("counts entities as one character and never straddles tags", () => {
    const html = 'x &lt;<span class="k">ab</span>c';
    // text is "x <abc"; mark "<ab" (2..5)
    const out = markRanges(html, [[2, 5]], "m");
    expect(text(out)).toBe("x <abc");
    expect(marked(out)).toEqual(["<", "ab"]);
    expect(out).toBe('x <mark class="m">&lt;</mark><span class="k"><mark class="m">ab</mark></span>c');
  });

  it("leaves html alone without ranges", () => {
    expect(markRanges("<b>x</b>", [], "m")).toBe("<b>x</b>");
  });
});

describe("renderDiff", () => {
  it("highlights both sides and marks changed words in paired lines", () => {
    const diff: FileDiff = {
      path: "src/lib.rs",
      oldPath: null,
      status: "modified",
      binary: false,
      truncated: false,
      hunks: [
        {
          header: "@@ -1,3 +1,3 @@",
          lines: [
            { kind: "context", oldLine: 1, newLine: 1, content: "fn open() {" },
            { kind: "delete", oldLine: 2, newLine: null, content: "    let repo = discover(path)?;" },
            { kind: "add", oldLine: null, newLine: 2, content: "    let repo = discover(path).map_err(wrap)?;" },
            { kind: "context", oldLine: 3, newLine: 3, content: "}" },
          ],
        },
      ],
    };
    const html = renderDiff(diff);
    const [ctx, del, add] = diff.hunks[0].lines;
    expect(html.get(ctx)).toContain("hljs-keyword");
    expect(marked(html.get(del)!)).toEqual([]);
    expect(marked(html.get(add)!).join("")).toBe(".map_err(wrap)");
    for (const line of diff.hunks[0].lines) expect(text(html.get(line)!)).toBe(line.content);
  });

  it("pairs a changed line with its most similar replacement, not by position", () => {
    const line = (kind: "delete" | "add", content: string) => ({ kind, oldLine: null, newLine: null, content });
    const lines = [
      line("delete", "pub fn open(path: &Path) -> Result<Repo, Error> {"),
      line("delete", "    let repo = Repo::discover(path)?;"),
      line("add", "/// Opens the repository containing `path`."),
      line("add", "pub fn open(path: &Path) -> Result<Repo, LoadError> {"),
      line("add", "    let repo = Repo::discover(path).map_err(LoadError::NotARepo)?;"),
      line("add", "    if repo.is_bare() {"),
    ];
    const diff: FileDiff = { path: "loader.rs", oldPath: null, status: "modified", binary: false, truncated: false, hunks: [{ header: "@@", lines }] };
    const html = renderDiff(diff);
    expect(marked(html.get(lines[0])!)).toEqual(["Error"]);
    expect(marked(html.get(lines[3])!)).toEqual(["LoadError"]);
    expect(marked(html.get(lines[4])!).join("")).toBe(".map_err(LoadError::NotARepo)");
    expect(marked(html.get(lines[2])!)).toEqual([]);
    expect(marked(html.get(lines[5])!)).toEqual([]);
  });

  it("escapes content in unknown file types", () => {
    const diff: FileDiff = {
      path: "notes",
      oldPath: null,
      status: "added",
      binary: false,
      truncated: false,
      hunks: [{ header: "@@", lines: [{ kind: "add", oldLine: null, newLine: 1, content: "<script>" }] }],
    };
    expect([...renderDiff(diff).values()]).toEqual([escapeHtml("<script>")]);
  });
});
