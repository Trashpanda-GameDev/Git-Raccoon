// Tests for the feedback relay, with Node's own test runner: npm test.

import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { generateKeyPairSync, createPublicKey, verify } from "node:crypto";

import { handle, reset, appJwt, pkcs8, checkIssue, checkScreenshot } from "../src/worker.js";

// A key as GitHub hands them out: PKCS #1.
const { privateKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
const pem = privateKey.export({ type: "pkcs1", format: "pem" });
const env = {
  APP_ID: "1234",
  INSTALLATION_ID: "5678",
  PRIVATE_KEY: pem,
  ISSUES_REPO: "owner/app",
  IMAGES_REPO: "owner/images",
};
const PNG = Buffer.from("\x89PNG\r\n\x1a\nrest of the file", "binary").toString("base64");

/// A stand-in for GitHub: records requests, answers by path.
function github() {
  const seen = [];
  const fetchImpl = async (url, init) => {
    seen.push({ url, init, body: init.body ? JSON.parse(init.body) : null });
    if (url.endsWith("/access_tokens")) {
      return Response.json({ token: "inst-token", expires_at: new Date(Date.now() + 3600e3).toISOString() }, { status: 201 });
    }
    if (url.endsWith("/issues")) {
      return Response.json({ number: 77, html_url: "https://github.com/owner/app/issues/77" }, { status: 201 });
    }
    if (url.includes("/contents/")) {
      return Response.json({ content: { download_url: "https://raw.example/shot.png" } }, { status: 201 });
    }
    return new Response("{}", { status: 404 });
  };
  return { seen, fetchImpl };
}

function post(path, json, ip = "1.2.3.4") {
  return new Request(`https://relay.example${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json", "CF-Connecting-IP": ip },
    body: typeof json === "string" ? json : JSON.stringify(json),
  });
}

const issue = { title: "Bug: It crashed", body: "When I clicked.", labels: ["bug"] };

beforeEach(() => reset());

test("posts an issue as the App's installation", async () => {
  const gh = github();
  const response = await handle(post("/issue", issue), env, gh.fetchImpl);
  assert.equal(response.status, 201);
  assert.deepEqual(await response.json(), { number: 77, url: "https://github.com/owner/app/issues/77" });
  // First a token for the installation, signed as the App…
  assert.equal(gh.seen[0].url, "https://api.github.com/app/installations/5678/access_tokens");
  assert.match(gh.seen[0].init.headers.Authorization, /^Bearer [\w-]+\.[\w-]+\.[\w-]+$/);
  // …then the issue, with it.
  assert.equal(gh.seen[1].url, "https://api.github.com/repos/owner/app/issues");
  assert.equal(gh.seen[1].init.headers.Authorization, "Bearer inst-token");
  assert.deepEqual(gh.seen[1].body, issue);
});

test("keeps the installation token for the next post", async () => {
  const gh = github();
  await handle(post("/issue", issue), env, gh.fetchImpl);
  await handle(post("/issue", issue), env, gh.fetchImpl);
  assert.equal(gh.seen.filter((r) => r.url.endsWith("/access_tokens")).length, 1);
});

test("uploads a screenshot", async () => {
  const gh = github();
  const response = await handle(post("/screenshot", { png: PNG }), env, gh.fetchImpl);
  assert.equal(response.status, 201);
  assert.deepEqual(await response.json(), { url: "https://raw.example/shot.png" });
  const put = gh.seen[1];
  assert.equal(put.init.method, "PUT");
  assert.match(put.url, /^https:\/\/api\.github\.com\/repos\/owner\/images\/contents\/.+\.png$/);
  assert.equal(put.body.content, PNG);
});

test("signs a JWT GitHub can check with the App's public key", async () => {
  const jwt = await appJwt(env, () => 1_700_000_000_000);
  const [header, claims, signature] = jwt.split(".");
  const decode = (s) => JSON.parse(Buffer.from(s, "base64url").toString());
  assert.deepEqual(decode(header), { alg: "RS256", typ: "JWT" });
  assert.deepEqual(decode(claims), { iat: 1_699_999_940, exp: 1_700_000_540, iss: "1234" });
  const ok = verify(
    "sha256",
    Buffer.from(`${header}.${claims}`),
    createPublicKey(privateKey),
    Buffer.from(signature, "base64url"),
  );
  assert.ok(ok);
});

test("takes the key as PKCS #8 too", () => {
  const p8 = privateKey.export({ type: "pkcs8", format: "pem" });
  // Wrapping the PKCS #1 key gives the same PKCS #8 bytes.
  assert.deepEqual(Buffer.from(pkcs8(pem)), Buffer.from(pkcs8(p8)));
});

test("turns away what isn't feedback", async () => {
  const gh = github();
  const status = async (path, json) => (await handle(post(path, json), env, gh.fetchImpl)).status;
  assert.equal(await status("/issue", "not json"), 400);
  assert.equal(await status("/issue", { ...issue, labels: ["wontfix"] }), 400);
  assert.equal(await status("/issue", { ...issue, labels: ["bug", "feedback"] }), 400);
  assert.equal(await status("/issue", { ...issue, title: " " }), 400);
  assert.equal(await status("/issue", { ...issue, body: "x".repeat(20_001) }), 400);
  assert.equal(await status("/screenshot", { png: Buffer.from("GIF89a....").toString("base64") }), 400);
  assert.equal(await status("/elsewhere", issue), 404);
  const get = await handle(new Request("https://relay.example/issue"), env, gh.fetchImpl);
  assert.equal(get.status, 405);
  assert.equal(gh.seen.length, 0, "GitHub was never asked");
});

test("limits how often one address posts", async () => {
  const gh = github();
  let at = 0;
  const now = () => at;
  const statuses = [];
  for (let i = 0; i < 7; i++) {
    statuses.push((await handle(post("/issue", issue, "9.9.9.9"), env, gh.fetchImpl, now)).status);
  }
  assert.deepEqual(statuses, [201, 201, 201, 201, 201, 201, 429]);
  // Another address isn't held up, and an hour later this one can again.
  assert.equal((await handle(post("/issue", issue, "8.8.8.8"), env, gh.fetchImpl, now)).status, 201);
  at += 61 * 60 * 1000;
  assert.equal((await handle(post("/issue", issue, "9.9.9.9"), env, gh.fetchImpl, now)).status, 201);
});

test("says so when GitHub won't give a token", async () => {
  const fetchImpl = async () => new Response("{}", { status: 401 });
  const response = await handle(post("/issue", issue), env, fetchImpl);
  assert.equal(response.status, 502);
  assert.match((await response.json()).error, /401/);
});

test("checks the parts on their own", () => {
  assert.equal(checkIssue(issue), null);
  assert.equal(checkScreenshot({ png: PNG }), null);
  assert.ok(checkScreenshot({ png: "!!!" }));
});
