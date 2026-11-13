// Mirrors the structs serialized by src-tauri/src/repo.rs and graph.rs.

export type RefKind = "head" | "local" | "remote" | "tag";

export type RefLabel = {
  name: string;
  kind: RefKind;
  isHead: boolean;
};

export type Edge = {
  from: number;
  to: number;
  lane: number;
};

export type GraphCommit = {
  id: string;
  parents: string[];
  summary: string;
  author: string;
  time: number;
  refs: RefLabel[];
  column: number;
  edges: Edge[];
};

export type GraphPage = {
  snapshot: number;
  offset: number;
  commits: GraphCommit[];
  total: number;
  width: number;
  branch: string | null;
  path: string;
};

// Mirrors src-tauri/src/details.rs.

export type FileStatus = "added" | "modified" | "deleted" | "renamed" | "copied" | "typechange";

export type ChangedFile = {
  path: string;
  oldPath: string | null;
  status: FileStatus;
  additions: number;
  deletions: number;
  binary: boolean;
};

export type Person = {
  name: string;
  email: string;
  time: number;
};

export type CommitDetails = {
  id: string;
  parents: string[];
  summary: string;
  body: string;
  author: Person;
  committer: Person;
  files: ChangedFile[];
  additions: number;
  deletions: number;
};

export type LineKind = "context" | "add" | "delete" | "noeol";

export type DiffLine = {
  kind: LineKind;
  oldLine: number | null;
  newLine: number | null;
  content: string;
};

export type Hunk = {
  header: string;
  lines: DiffLine[];
};

export type FileDiff = {
  path: string;
  oldPath: string | null;
  status: FileStatus;
  binary: boolean;
  hunks: Hunk[];
  truncated: boolean;
};

// Mirrors src-tauri/src/refs.rs.

export type Upstream = {
  name: string;
  ahead: number;
  behind: number;
  gone: boolean;
};

export type LocalBranch = {
  name: string;
  target: string;
  isHead: boolean;
  upstream: Upstream | null;
};

export type RemoteBranch = {
  name: string;
  fullName: string;
  target: string;
};

export type Remote = {
  name: string;
  url: string | null;
  branches: RemoteBranch[];
};

export type Tag = {
  name: string;
  target: string;
  annotated: boolean;
};

export type Stash = {
  index: number;
  message: string;
  target: string;
};

export type Refs = {
  local: LocalBranch[];
  remotes: Remote[];
  tags: Tag[];
  stashes: Stash[];
};

// Mirrors src-tauri/src/search.rs.

export type SearchField = "text" | "path";

export type SearchResult = {
  rows: number[];
  next: number | null;
  cancelled: boolean;
};
