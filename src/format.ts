const relativeTime = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 31536000],
  ["month", 2592000],
  ["week", 604800],
  ["day", 86400],
  ["hour", 3600],
  ["minute", 60],
];

/** "3 days ago", "yesterday", "just now". */
export function formatAge(seconds: number) {
  const diff = seconds - Date.now() / 1000;
  for (const [unit, size] of UNITS) {
    if (Math.abs(diff) >= size) return relativeTime.format(Math.round(diff / size), unit);
  }
  return "just now";
}

const dateTime = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

export const formatDateTime = (seconds: number) => dateTime.format(new Date(seconds * 1000));

export const shortSha = (id: string) => id.slice(0, 7);

/** Splits "src/app/main.ts" into ["src/app/", "main.ts"]. */
export function splitPath(path: string): [string, string] {
  const i = path.lastIndexOf("/");
  return i < 0 ? ["", path] : [path.slice(0, i + 1), path.slice(i + 1)];
}
