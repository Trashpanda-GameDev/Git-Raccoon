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
