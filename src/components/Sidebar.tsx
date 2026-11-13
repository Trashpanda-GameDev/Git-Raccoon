import { useMemo, useState, type ReactNode } from "react";
import type { LocalBranch, Refs, RemoteBranch, Stash, Tag } from "../types";

type TreeNode<T> =
  | { kind: "folder"; name: string; path: string; children: TreeNode<T>[] }
  | { kind: "leaf"; name: string; item: T };

/** Groups "feature/a", "feature/b", "main" into a "feature" folder and "main". */
export function buildTree<T>(items: T[], nameOf: (item: T) => string): TreeNode<T>[] {
  const root: TreeNode<T>[] = [];
  for (const item of items) {
    const parts = nameOf(item).split("/");
    let level = root;
    let path = "";
    for (const part of parts.slice(0, -1)) {
      path = path ? `${path}/${part}` : part;
      let folder = level.find((n): n is Extract<TreeNode<T>, { kind: "folder" }> => n.kind === "folder" && n.name === part);
      if (!folder) {
        folder = { kind: "folder", name: part, path, children: [] };
        level.push(folder);
      }
      level = folder.children;
    }
    level.push({ kind: "leaf", name: parts[parts.length - 1], item });
  }
  const sort = (nodes: TreeNode<T>[]) => {
    // Folders first, each group keeping the incoming order.
    nodes.sort((a, b) => (a.kind === b.kind ? 0 : a.kind === "folder" ? -1 : 1));
    for (const n of nodes) if (n.kind === "folder") sort(n.children);
  };
  sort(root);
  return root;
}

type Props = {
  refs: Refs | null;
  selected: string | null;
  onJump: (commitId: string) => void;
};

const matches = (filter: string, ...texts: string[]) =>
  !filter || texts.some((t) => t.toLowerCase().includes(filter));

export function Sidebar({ refs, selected, onJump }: Props) {
  const [filter, setFilter] = useState("");
  const [closed, setClosed] = useState<Set<string>>(() => new Set());
  const f = filter.trim().toLowerCase();

  const toggle = (key: string) =>
    setClosed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  // While filtering, show every match regardless of collapsed folders.
  const isOpen = (key: string) => f !== "" || !closed.has(key);

  const local = useMemo(() => refs?.local.filter((b) => matches(f, b.name)) ?? [], [refs, f]);
  const remotes = useMemo(
    () =>
      refs?.remotes
        .map((r) => ({ ...r, branches: r.branches.filter((b) => matches(f, b.fullName)) }))
        .filter((r) => !f || r.branches.length > 0 || matches(f, r.name)) ?? [],
    [refs, f],
  );
  const tags = useMemo(() => refs?.tags.filter((t) => matches(f, t.name)) ?? [], [refs, f]);
  const stashes = useMemo(() => refs?.stashes.filter((s) => matches(f, s.message, `stash@{${s.index}}`)) ?? [], [refs, f]);

  function renderTree<T>(nodes: TreeNode<T>[], keyPrefix: string, depth: number, renderLeaf: (item: T, name: string, depth: number) => ReactNode): ReactNode {
    return nodes.map((node) => {
      if (node.kind === "leaf") return renderLeaf(node.item, node.name, depth);
      const key = `${keyPrefix}/${node.path}`;
      const open = isOpen(key);
      return (
        <li key={key}>
          <button type="button" className="side-row folder" style={{ paddingLeft: 12 + depth * 14 }} onClick={() => toggle(key)} aria-expanded={open}>
            <span className={`caret${open ? " open" : ""}`}>▸</span>
            <span className="side-icon">📁</span>
            <span className="side-name">{node.name}</span>
          </button>
          {open && <ul>{renderTree(node.children, keyPrefix, depth + 1, renderLeaf)}</ul>}
        </li>
      );
    });
  }

  const leaf = (key: string, target: string, depth: number, content: ReactNode, title: string, extraClass = "") => (
    <li key={key}>
      <button
        type="button"
        className={`side-row${target === selected ? " active" : ""}${extraClass}`}
        style={{ paddingLeft: 26 + depth * 14 }}
        onClick={() => onJump(target)}
        title={title}
      >
        {content}
      </button>
    </li>
  );

  const localLeaf = (b: LocalBranch, name: string, depth: number) => {
    const up = b.upstream;
    const title = up
      ? up.gone
        ? `${b.name}\nUpstream ${up.name} is gone`
        : `${b.name}\nTracking ${up.name}: ${up.ahead} ahead, ${up.behind} behind`
      : `${b.name}\nNo upstream`;
    return leaf(
      `local/${b.name}`,
      b.target,
      depth,
      <>
        <span className="side-icon">{b.isHead ? "✓" : "⎇"}</span>
        <span className="side-name">{name}</span>
        {up && !up.gone && up.ahead > 0 && <span className="badge ahead">↑{up.ahead}</span>}
        {up && !up.gone && up.behind > 0 && <span className="badge behind">↓{up.behind}</span>}
        {up?.gone && <span className="badge gone">gone</span>}
      </>,
      title,
      b.isHead ? " current" : "",
    );
  };

  const remoteLeaf = (b: RemoteBranch, name: string, depth: number) =>
    leaf(
      `remote/${b.fullName}`,
      b.target,
      depth,
      <>
        <span className="side-icon">⎇</span>
        <span className="side-name">{name}</span>
      </>,
      b.fullName,
    );

  const tagLeaf = (t: Tag, name: string, depth: number) =>
    leaf(
      `tag/${t.name}`,
      t.target,
      depth,
      <>
        <span className="side-icon">⌂</span>
        <span className="side-name">{name}</span>
      </>,
      `${t.name}${t.annotated ? " (annotated)" : ""}`,
    );

  const stashLeaf = (s: Stash) =>
    leaf(
      `stash/${s.index}`,
      s.target,
      0,
      <>
        <span className="side-icon">⧉</span>
        <span className="side-name">{s.message}</span>
      </>,
      `stash@{${s.index}}: ${s.message}`,
    );

  const section = (key: string, title: string, count: number, body: ReactNode, empty: string, hasItems = count > 0) => {
    const open = isOpen(key);
    return (
      <section className="side-section">
        <button type="button" className="side-header" onClick={() => toggle(key)} aria-expanded={open}>
          <span className={`caret${open ? " open" : ""}`}>▸</span>
          <span>{title}</span>
          <span className="side-count">{count}</span>
        </button>
        {open && (hasItems ? <ul>{body}</ul> : <div className="side-empty muted">{f ? "No matches" : empty}</div>)}
      </section>
    );
  };

  const remoteBranchCount = remotes.reduce((n, r) => n + r.branches.length, 0);

  return (
    <nav className="sidebar" aria-label="Branches, remotes, tags and stashes">
      <div className="side-filter">
        <input value={filter} onChange={(e) => setFilter(e.currentTarget.value)} placeholder="Filter…" spellCheck={false} aria-label="Filter refs" />
      </div>
      <div className="side-scroll">
        {section("local", "Local", local.length, renderTree(buildTree(local, (b) => b.name), "local", 0, localLeaf), "No branches")}
        {section(
          "remotes",
          "Remote",
          remoteBranchCount,
          remotes.map((r) => {
            const key = `remote:${r.name}`;
            const open = isOpen(key);
            return (
              <li key={key}>
                <button type="button" className="side-row folder" style={{ paddingLeft: 12 }} onClick={() => toggle(key)} title={r.url ?? r.name} aria-expanded={open}>
                  <span className={`caret${open ? " open" : ""}`}>▸</span>
                  <span className="side-icon">☁</span>
                  <span className="side-name">{r.name}</span>
                </button>
                {open && <ul>{renderTree(buildTree(r.branches, (b) => b.name), key, 1, remoteLeaf)}</ul>}
              </li>
            );
          }),
          "No remotes",
          remotes.length > 0,
        )}
        {section("tags", "Tags", tags.length, tags.map((t) => tagLeaf(t, t.name, 0)), "No tags")}
        {section("stashes", "Stashes", stashes.length, stashes.map(stashLeaf), "No stashes")}
      </div>
    </nav>
  );
}
