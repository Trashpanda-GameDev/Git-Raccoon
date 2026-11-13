// Recently opened repositories, newest first, kept in local storage.

export type RecentRepo = {
  path: string;
  name: string;
  openedAt: number;
};

const KEY = "git-raccoon:recent-repos";
const MAX = 12;

export const repoName = (path: string) => path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path;

export function loadRecent(): RecentRepo[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter((r) => typeof r?.path === "string") : [];
  } catch {
    return [];
  }
}

function save(list: RecentRepo[]) {
  try {
    localStorage.setItem(KEY, JSON.stringify(list));
  } catch {
    // Remembering repos is a convenience.
  }
}

/** Moves `path` to the front of the list, adding it if new. */
export function rememberRepo(path: string): RecentRepo[] {
  const clean = path.replace(/[\\/]+$/, "");
  const list = [{ path: clean, name: repoName(clean), openedAt: Date.now() }, ...loadRecent().filter((r) => r.path !== clean)].slice(0, MAX);
  save(list);
  return list;
}

export function forgetRepo(path: string): RecentRepo[] {
  const list = loadRecent().filter((r) => r.path !== path);
  save(list);
  return list;
}
