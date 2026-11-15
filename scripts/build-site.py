#!/usr/bin/env python3
"""Builds the project website into _site/: the pages in site/, the user guide
(docs/guide.md, rendered into site/guide.html), and the app icon.

    pip install markdown
    python3 scripts/build-site.py
    python3 -m http.server -d _site    # to look at it: http://localhost:8000

The GitHub Pages workflow (.github/workflows/pages.yml) runs it and
publishes _site/. The screenshots in site/screenshots/ come from the app
itself: cargo test -p git-raccoon --test site_screenshots -- --ignored
"""

import html
import pathlib
import shutil

import markdown

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
OUT = ROOT / "_site"


def main():
    if OUT.exists():
        shutil.rmtree(OUT)
    shutil.copytree(SITE, OUT)
    shutil.copy(ROOT / "crates/git-raccoon/assets/icon.png", OUT / "icon.png")

    md = markdown.Markdown(
        extensions=["tables", "toc", "sane_lists"],
        extension_configs={
            "toc": {"permalink": "#", "permalink_class": "anchor", "toc_depth": "2"}
        },
    )
    content = md.convert((ROOT / "docs/guide.md").read_text(encoding="utf-8"))
    sections = [t for t in md.toc_tokens[0]["children"]] if md.toc_tokens else []
    toc = "\n".join(
        f'      <li><a href="#{t["id"]}">{html.escape(html.unescape(t["name"]))}</a></li>'
        for t in sections
    )
    template = (SITE / "guide.html").read_text(encoding="utf-8")
    page = template.replace("{{toc}}", toc).replace("{{content}}", content)
    (OUT / "guide.html").write_text(page, encoding="utf-8")
    print(f"{OUT}: {len(sections)} guide sections")


if __name__ == "__main__":
    main()
