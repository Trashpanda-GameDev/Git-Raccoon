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

export type Graph = {
  commits: GraphCommit[];
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
