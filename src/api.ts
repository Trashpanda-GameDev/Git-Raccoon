import { invoke } from "@tauri-apps/api/core";
import type { CommitDetails, FileDiff, GraphPage, Refs } from "./types";

export const loadGraph = (path: string, limit: number) => invoke<GraphPage>("load_graph", { path, limit });

export const graphPage = (snapshot: number, offset: number, limit: number) =>
  invoke<GraphPage>("graph_page", { snapshot, offset, limit });

export const locateCommit = (snapshot: number, id: string) => invoke<number | null>("locate_commit", { snapshot, id });

export const loadRefs = (path: string) => invoke<Refs>("load_refs", { path });

export const loadCommitDetails = (path: string, id: string) =>
  invoke<CommitDetails>("commit_details", { path, id });

export const loadFileDiff = (path: string, id: string, file: string) =>
  invoke<FileDiff>("file_diff", { path, id, file });
