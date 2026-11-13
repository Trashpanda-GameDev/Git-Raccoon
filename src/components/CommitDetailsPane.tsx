import { useState } from "react";
import type { ChangedFile, CommitDetails, FileStatus, Person } from "../types";
import { formatAge, formatDateTime, shortSha, splitPath } from "../format";

const STATUS_LETTERS: Record<FileStatus, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  copied: "C",
  typechange: "T",
};

export function StatusBadge({ status }: { status: FileStatus }) {
  return (
    <span className={`status-badge status-${status}`} title={status}>
      {STATUS_LETTERS[status]}
    </span>
  );
}

/**
 * Commit messages are usually hard-wrapped at ~72 columns. Join wrapped lines
 * back into paragraphs so they reflow, but keep blank-line paragraph breaks,
 * list items and indented (code) lines on their own lines.
 */
export function unwrapParagraphs(body: string): string[] {
  return body.split(/\n\s*\n/).map((para) =>
    para
      .split("\n")
      .reduce((out, line) => {
        const keepBreak = /^\s*([-*+•]|\d+[.)])\s/.test(line) || /^\s{2,}|^\t/.test(line);
        if (out === "") return line;
        return keepBreak ? `${out}\n${line}` : `${out} ${line.trim()}`;
      }, ""),
  );
}

function initials(name: string) {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  return (parts[0][0] + (parts.length > 1 ? parts[parts.length - 1][0] : "")).toUpperCase();
}

function avatarHue(email: string) {
  let h = 0;
  for (const ch of email) h = (h * 31 + ch.charCodeAt(0)) % 360;
  return h;
}

function PersonRow({ person, label }: { person: Person; label: string }) {
  return (
    <div className="person">
      <span className="avatar" style={{ background: `hsl(${avatarHue(person.email)} 45% 40%)` }} aria-hidden="true">
        {initials(person.name)}
      </span>
      <div className="person-text">
        <div>
          <span className="person-name">{person.name}</span>{" "}
          <span className="muted">{label}</span>
        </div>
        <div className="muted" title={person.email}>
          {formatDateTime(person.time)} · {formatAge(person.time)}
        </div>
      </div>
    </div>
  );
}

function ChangeBar({ additions, deletions }: { additions: number; deletions: number }) {
  const total = additions + deletions;
  if (total === 0) return null;
  const blocks = 5;
  const adds = Math.round((additions / total) * blocks);
  return (
    <span className="change-bar" aria-hidden="true">
      {Array.from({ length: blocks }, (_, i) => (
        <span key={i} className={i < adds ? "bar-add" : "bar-del"} />
      ))}
    </span>
  );
}

function FileRow({ file, active, onOpen }: { file: ChangedFile; active: boolean; onOpen: () => void }) {
  const [dir, name] = splitPath(file.path);
  return (
    <li>
      <button type="button" className={`file-row${active ? " active" : ""}`} onClick={onOpen} title={file.path}>
        <StatusBadge status={file.status} />
        <span className="file-path">
          <span className="muted file-dir">
            {file.oldPath && `${file.oldPath} → `}
            {dir}
          </span>
          <span className="file-name">{name}</span>
        </span>
        {file.binary ? (
          <span className="muted file-counts">binary</span>
        ) : (
          <span className="file-counts">
            {file.additions > 0 && <span className="add">+{file.additions}</span>}
            {file.deletions > 0 && <span className="del">−{file.deletions}</span>}
          </span>
        )}
      </button>
    </li>
  );
}

type Props = {
  details: CommitDetails | null;
  loading: boolean;
  error: string;
  activeFile: string | null;
  onOpenFile: (path: string) => void;
  onSelectCommit: (id: string) => void;
  onClose: () => void;
};

export function CommitDetailsPane({ details, loading, error, activeFile, onOpenFile, onSelectCommit, onClose }: Props) {
  const [copied, setCopied] = useState(false);

  async function copySha() {
    if (!details) return;
    try {
      await navigator.clipboard.writeText(details.id);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      // Clipboard access can be refused; the SHA is still selectable.
    }
  }

  return (
    <aside className="details-pane" aria-label="Commit details">
      <div className="pane-header">
        <span className="pane-title">Commit</span>
        {details && (
          <button type="button" className="sha-button" onClick={copySha} title="Copy full SHA">
            <code>{shortSha(details.id)}</code> {copied ? "✓ copied" : "⧉"}
          </button>
        )}
        <button type="button" className="icon-button close" onClick={onClose} title="Close (Esc)" aria-label="Close details">
          ×
        </button>
      </div>

      {error && <div className="error">{error}</div>}
      {!details && loading && <div className="pane-empty muted">Loading…</div>}

      {details && (
        <div className={`pane-body${loading ? " stale" : ""}`}>
          <h2 className="commit-summary">{details.summary || <span className="muted">(no message)</span>}</h2>
          {details.body &&
            unwrapParagraphs(details.body).map((p, i) => (
              <p key={i} className="commit-body">
                {p}
              </p>
            ))}

          <PersonRow person={details.author} label="authored" />
          {(details.committer.name !== details.author.name || details.committer.email !== details.author.email) && (
            <PersonRow person={details.committer} label="committed" />
          )}

          {details.parents.length > 0 && (
            <div className="parents">
              <span className="muted">{details.parents.length > 1 ? "Parents" : "Parent"}</span>
              {details.parents.map((p) => (
                <button key={p} type="button" className="link-button" onClick={() => onSelectCommit(p)} title={p}>
                  <code>{shortSha(p)}</code>
                </button>
              ))}
              {details.parents.length > 1 && <span className="muted">· changes vs first parent</span>}
            </div>
          )}

          <div className="files-summary">
            <span>
              {details.files.length} {details.files.length === 1 ? "file" : "files"} changed
            </span>
            <span className="add">+{details.additions}</span>
            <span className="del">−{details.deletions}</span>
            <ChangeBar additions={details.additions} deletions={details.deletions} />
          </div>

          <ul className="file-list">
            {details.files.map((f) => (
              <FileRow key={f.path} file={f} active={f.path === activeFile} onOpen={() => onOpenFile(f.path)} />
            ))}
          </ul>
        </div>
      )}
    </aside>
  );
}
