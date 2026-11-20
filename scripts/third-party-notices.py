#!/usr/bin/env python3
"""Writes crates/git-raccoon/assets/THIRD-PARTY-NOTICES.txt: the licenses of
everything compiled into the app.

It lists every crate the app is built from (not build tools or test-only
crates), with each license text found in its sources, grouped so a text
shared by many crates appears once; then the C libraries and other assets
bundled inside some of them (libgit2, libssh2, zlib, the fonts), which
Cargo doesn't know about. The syntax definitions and themes are credited in
the app itself, from two-face's own acknowledgements.

Run it from the repository root after dependencies change:

    python3 scripts/third-party-notices.py
"""

import json
import pathlib
import subprocess
import sys
from collections import defaultdict

OUT = pathlib.Path("crates/git-raccoon/assets/THIRD-PARTY-NOTICES.txt")
OURS = {"git-raccoon", "git-raccoon-core"}
LICENSE_NAMES = ("license", "licence", "copying", "notice", "unlicense")

# C libraries and assets bundled inside crates: (what, crate, files in it).
BUNDLED = [
    ("libgit2 (Git in C), bundled by libgit2-sys", "libgit2-sys", ["libgit2/COPYING"]),
    ("llhttp (HTTP parsing in libgit2)", "libgit2-sys", ["libgit2/deps/llhttp/LICENSE-MIT"]),
    ("PCRE2 (regular expressions in libgit2)", "libgit2-sys", ["libgit2/deps/pcre2/LICENCE.md"]),
    ("libssh2 (SSH), bundled by libssh2-sys", "libssh2-sys", ["libssh2/COPYING"]),
    ("zlib (compression), bundled by libz-sys", "libz-sys", ["src/zlib/LICENSE"]),
    (
        "Fonts bundled by epaint_default_fonts: Ubuntu Light (Ubuntu Font Licence), "
        "Hack and Noto Emoji (SIL Open Font License), emoji-icon-font (MIT)",
        "epaint_default_fonts",
        ["fonts/UFL.txt", "fonts/OFL.txt", "fonts/emoji-icon-font-mit-license.txt"],
    ),
]


# Assets of our own from elsewhere: (what, files in this repository).
OWN_BUNDLED = [
    (
        "Ubuntu (variable font, for bold text), from the Ubuntu font family "
        "(Ubuntu Font Licence)",
        ["crates/git-raccoon/assets/fonts/UFL.txt"],
    ),
]


def metadata():
    # The sources of every platform's crates, so their license files can be read.
    subprocess.run(["cargo", "fetch", "--locked"], check=True)
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return json.loads(out)


def shipped(meta):
    """Packages reachable from the app through normal (not build or dev) deps."""
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = next(p["id"] for p in meta["packages"] if p["name"] == "git-raccoon")
    seen, stack = set(), [root]
    while stack:
        node = stack.pop()
        if node in seen:
            continue
        seen.add(node)
        for dep in nodes[node]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    found = [packages[i] for i in seen if packages[i]["name"] not in OURS]
    return sorted(found, key=lambda p: (p["name"], p["version"]))


def license_texts(package):
    root = pathlib.Path(package["manifest_path"]).parent
    texts = []
    for path in sorted(root.iterdir()):
        if path.is_file() and path.name.lower().startswith(LICENSE_NAMES):
            try:
                texts.append(path.read_text(encoding="utf-8", errors="replace").strip())
            except OSError:
                pass
    return texts


def main():
    meta = metadata()
    packages = shipped(meta)
    by_text = defaultdict(list)
    without = []
    for p in packages:
        label = f"{p['name']} {p['version']} ({p.get('license') or 'see its sources'})"
        texts = license_texts(p)
        if not texts:
            without.append(label)
        for text in texts:
            by_text[text].append(label)

    roots = {p["name"]: pathlib.Path(p["manifest_path"]).parent for p in packages}
    lines = [
        "THIRD-PARTY NOTICES",
        "",
        "Git Raccoon is built from the open-source software below. Each is",
        "used under its own license, reproduced here.",
        "",
        "=" * 72,
        "BUNDLED LIBRARIES AND FONTS",
        "=" * 72,
    ]
    for what, crate, files in BUNDLED:
        if crate not in roots:
            continue
        lines += ["", "-" * 72, what, "-" * 72]
        for f in files:
            path = roots[crate] / f
            if path.exists():
                lines += ["", path.read_text(encoding="utf-8", errors="replace").strip()]

    for what, files in OWN_BUNDLED:
        lines += ["", "-" * 72, what, "-" * 72]
        for f in files:
            lines += ["", pathlib.Path(f).read_text(encoding="utf-8").strip()]

    lines += ["", "=" * 72, f"RUST CRATES ({len(packages)})", "=" * 72]
    # The texts shared by the most crates first.
    for text, users in sorted(by_text.items(), key=lambda kv: (-len(kv[1]), kv[1][0])):
        lines += ["", "-" * 72, "Used by:"]
        lines += [f"  {u}" for u in users]
        lines += ["-" * 72, "", text]
    if without:
        lines += [
            "",
            "-" * 72,
            "These crates ship no license file; their license is as stated:",
        ]
        lines += [f"  {u}" for u in without]
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{OUT}: {len(packages)} crates, {len(by_text)} distinct license texts, "
          f"{len(without)} without a file", file=sys.stderr)


if __name__ == "__main__":
    main()
