import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type CommitSummary = {
  id: string;
  parents: string[];
  summary: string;
  author: string;
  time: number;
};

function App() {
  const [path, setPath] = useState("");
  const [commits, setCommits] = useState<CommitSummary[]>([]);
  const [error, setError] = useState("");

  async function openRepo() {
    setError("");
    try {
      setCommits(await invoke<CommitSummary[]>("log", { path, limit: 200 }));
    } catch (e) {
      setCommits([]);
      setError(String(e));
    }
  }

  return (
    <main className="container">
      <h1>Git Raccoon</h1>

      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          openRepo();
        }}
      >
        <input
          id="repo-path"
          onChange={(e) => setPath(e.currentTarget.value)}
          placeholder="Path to a Git repository..."
        />
        <button type="submit">Open</button>
      </form>

      {error && <p className="error">{error}</p>}

      <ul className="commits">
        {commits.map((c) => (
          <li key={c.id}>
            <code>{c.id.slice(0, 7)}</code> {c.summary} <em>{c.author}</em>
          </li>
        ))}
      </ul>
    </main>
  );
}

export default App;
