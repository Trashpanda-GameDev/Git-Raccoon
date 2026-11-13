// Small per-user preferences kept in local storage. Storage can be
// unavailable, so reads fall back to "" and failed writes are ignored.

export const LAST_REPO_KEY = "git-raccoon:last-repo";
export const DIFF_MODE_KEY = "git-raccoon:diff-mode";
export const SIDEBAR_KEY = "git-raccoon:sidebar";

export function readSetting(key: string) {
  try {
    return localStorage.getItem(key) ?? "";
  } catch {
    return "";
  }
}

export function writeSetting(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Settings are a convenience.
  }
}
