// Git Raccoon's feedback relay: a Cloudflare Worker that posts feedback
// from the app as a GitHub App ("Git Raccoon Feedback"), so the app ships
// no token of its own.
//
// It does two things, and nothing else:
//
//   POST /issue       {title, body, labels}  -> an issue in ISSUES_REPO,
//                                                answered {number, url}
//   POST /screenshot  {png}  (base64)        -> a file in IMAGES_REPO,
//                                                answered {url}
//
// What it's sent is checked (sizes, labels, that a screenshot is a PNG),
// and each address may post only so often. The App's private key, its id
// and its installation's id are the Worker's settings (see README.md).

const API = "https://api.github.com";
/// The labels feedback may carry: one of these, exactly.
const LABELS = new Set(["bug", "feedback", "enhancement"]);
const MAX_TITLE = 256;
const MAX_BODY = 20_000;
/// Screenshots, as sent (base64): about 5 MB of PNG.
const MAX_PNG_BASE64 = 7_000_000;
/// Per address, per hour: issues and screenshots each.
const PER_HOUR = { issue: 6, screenshot: 6 };

export default {
  async fetch(request, env) {
    return handle(request, env);
  },
};

/// Recent posts by address (this Worker instance's memory: a speed bump
/// against floods, not a guarantee).
const recent = new Map();

/// For tests: forget what's been posted, and the cached token and key.
export function reset() {
  recent.clear();
  cache.token = null;
  cache.key = null;
}

/// Answers a request. `fetchImpl` and `now` are for tests.
export async function handle(request, env, fetchImpl = fetch, now = Date.now) {
  const url = new URL(request.url);
  const route = { "/issue": "issue", "/screenshot": "screenshot" }[url.pathname];
  if (!route) {
    return reply(404, { error: "Not here." });
  }
  if (request.method !== "POST") {
    return reply(405, { error: "Only POST." });
  }
  const who = request.headers.get("CF-Connecting-IP") || "unknown";
  if (tooMany(`${route}:${who}`, PER_HOUR[route], now())) {
    return reply(429, { error: "Too many in an hour; try again later." });
  }
  let input;
  try {
    input = await request.json();
  } catch {
    return reply(400, { error: "That isn't JSON." });
  }
  const problem = route === "issue" ? checkIssue(input) : checkScreenshot(input);
  if (problem) {
    return reply(400, { error: problem });
  }
  let token;
  try {
    token = await installationToken(env, fetchImpl, now);
  } catch (e) {
    return reply(502, { error: `Couldn't sign in to GitHub: ${e.message}` });
  }
  return route === "issue"
    ? postIssue(input, env, token, fetchImpl)
    : postScreenshot(input, env, token, fetchImpl, now);
}

function reply(status, json) {
  return new Response(JSON.stringify(json), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function tooMany(key, limit, at) {
  const hour = 60 * 60 * 1000;
  const times = (recent.get(key) || []).filter((t) => at - t < hour);
  if (times.length >= limit) {
    recent.set(key, times);
    return true;
  }
  times.push(at);
  recent.set(key, times);
  return false;
}

/// What's wrong with an issue, if anything.
export function checkIssue(input) {
  if (!input || typeof input !== "object") return "Send {title, body, labels}.";
  const { title, body, labels } = input;
  if (typeof title !== "string" || !title.trim()) return "It needs a title.";
  if (title.length > MAX_TITLE) return "The title is too long.";
  if (typeof body !== "string") return "It needs a body.";
  if (body.length > MAX_BODY) return "The body is too long.";
  if (!Array.isArray(labels) || labels.length !== 1 || !LABELS.has(labels[0])) {
    return "It needs one label: bug, feedback or enhancement.";
  }
  return null;
}

/// What's wrong with a screenshot, if anything.
export function checkScreenshot(input) {
  if (!input || typeof input.png !== "string") return "Send {png} (base64).";
  if (input.png.length > MAX_PNG_BASE64) return "The screenshot is too big.";
  let start;
  try {
    start = atob(input.png.slice(0, 12));
  } catch {
    return "The screenshot isn't base64.";
  }
  if (!start.startsWith("\x89PNG")) return "The screenshot isn't a PNG.";
  return null;
}

function github(token, method, body) {
  return {
    method,
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: "application/vnd.github+json",
      "X-GitHub-Api-Version": "2022-11-28",
      "User-Agent": "git-raccoon-feedback-relay",
      "Content-Type": "application/json",
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  };
}

async function postIssue(input, env, token, fetchImpl) {
  const response = await fetchImpl(
    `${API}/repos/${env.ISSUES_REPO}/issues`,
    github(token, "POST", {
      title: input.title.trim(),
      body: input.body,
      labels: input.labels,
    }),
  );
  if (!response.ok) {
    return reply(502, { error: `GitHub answered ${response.status}.` });
  }
  const made = await response.json();
  return reply(201, { number: made.number, url: made.html_url });
}

async function postScreenshot(input, env, token, fetchImpl, now) {
  const stamp = new Date(now()).toISOString().replace(/[:.]/g, "-");
  const name = `${stamp}-${crypto.randomUUID().slice(0, 8)}.png`;
  const response = await fetchImpl(
    `${API}/repos/${env.IMAGES_REPO}/contents/${name}`,
    github(token, "PUT", {
      message: "A screenshot sent with feedback",
      content: input.png,
    }),
  );
  if (!response.ok) {
    return reply(502, { error: `GitHub answered ${response.status}.` });
  }
  const made = await response.json();
  return reply(201, { url: made.content.download_url });
}

/// The installation token, and the App's key, kept between requests.
const cache = { token: null, key: null };

/// A token to act as the App's installation: from GitHub, signed with the
/// App's key; reused until a few minutes before it runs out.
async function installationToken(env, fetchImpl, now) {
  if (cache.token && cache.token.expires - now() > 5 * 60 * 1000) {
    return cache.token.value;
  }
  const jwt = await appJwt(env, now);
  const response = await fetchImpl(
    `${API}/app/installations/${env.INSTALLATION_ID}/access_tokens`,
    github(jwt, "POST"),
  );
  if (!response.ok) {
    throw new Error(`GitHub answered ${response.status}`);
  }
  const made = await response.json();
  cache.token = { value: made.token, expires: Date.parse(made.expires_at) };
  return made.token;
}

/// A short-lived JSON Web Token saying "this is the App", signed with its
/// private key (RS256), as GitHub asks for.
export async function appJwt(env, now = Date.now) {
  if (!cache.key) {
    cache.key = await crypto.subtle.importKey(
      "pkcs8",
      pkcs8(env.PRIVATE_KEY),
      { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" },
      false,
      ["sign"],
    );
  }
  const seconds = Math.floor(now() / 1000);
  const header = { alg: "RS256", typ: "JWT" };
  // A minute back for clock drift; GitHub takes ten minutes at most.
  const claims = { iat: seconds - 60, exp: seconds + 9 * 60, iss: String(env.APP_ID) };
  const unsigned = `${base64url(JSON.stringify(header))}.${base64url(JSON.stringify(claims))}`;
  const signature = await crypto.subtle.sign(
    "RSASSA-PKCS1-v1_5",
    cache.key,
    new TextEncoder().encode(unsigned),
  );
  return `${unsigned}.${base64url(new Uint8Array(signature))}`;
}

function base64url(data) {
  const bytes = typeof data === "string" ? new TextEncoder().encode(data) : data;
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/// The key as PKCS #8, which is what Web Crypto takes. GitHub hands out
/// PKCS #1 ("BEGIN RSA PRIVATE KEY"): that's wrapped as it is, so the
/// .pem file can be pasted in unchanged.
export function pkcs8(pem) {
  const pkcs1 = pem.includes("BEGIN RSA PRIVATE KEY");
  const der = Uint8Array.from(
    atob(pem.replace(/-----[^-]+-----/g, "").replace(/\s+/g, "")),
    (c) => c.charCodeAt(0),
  );
  if (!pkcs1) return der;
  // PrivateKeyInfo { version 0, rsaEncryption, OCTET STRING { the key } }
  const algorithm = [0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01, 0x05, 0x00];
  const inner = [0x02, 0x01, 0x00, ...algorithm, 0x04, ...length(der.length), ...der];
  return Uint8Array.from([0x30, ...length(inner.length), ...inner]);
}

/// A DER length.
function length(n) {
  if (n < 0x80) return [n];
  const bytes = [];
  for (let m = n; m > 0; m >>= 8) bytes.unshift(m & 0xff);
  return [0x80 | bytes.length, ...bytes];
}
