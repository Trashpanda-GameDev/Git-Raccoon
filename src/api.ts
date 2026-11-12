import { invoke } from "@tauri-apps/api/core";
import type { CommitDetails, FileDiff, Graph } from "./types";

export const loadGraph = (path: string) => invoke<Graph>("load_graph", { path });

export const loadCommitDetails = (path: string, id: string) =>
  invoke<CommitDetails>("commit_details", { path, id });

export const loadFileDiff = (path: string, id: string, file: string) =>
  invoke<FileDiff>("file_diff", { path, id, file });
