# Feedback relay

Git Raccoon's feedback form posts a GitHub issue. Someone signed in to GitHub in the app posts it as themselves, with their own account. Everyone else's goes through this relay: a small Cloudflare Worker that posts it as the **Git Raccoon Feedback** GitHub App. The app itself ships no token. The relay also keeps screenshots, in the feedback-images repository, for both.

It answers two requests and nothing else:

| Request | Does |
|---|---|
| `POST /issue` with `{title, body, labels}` | Creates an issue in `ISSUES_REPO` as the App. Answers `{number, url}`. |
| `POST /screenshot` with `{png}` (base64) | Adds the PNG to `IMAGES_REPO`. Answers `{url}`. |

It checks what it's sent (one label out of `bug`, `feedback`, `enhancement`; sizes; that a screenshot really is a PNG) and lets each address post six of each an hour.

Tests: `npm test` (Node 22, no packages to install). CI runs them too.

## Setting it up

This needs a GitHub account that owns the repositories and a Cloudflare account. Both are free at this scale.

### 1. Create the GitHub App

1. Go to **GitHub › Settings › Developer settings › GitHub Apps › New GitHub App** (for an organisation: the organisation's settings, same place).
2. Fill in:
   - **GitHub App name:** `Git Raccoon Feedback` (issues show as `git-raccoon-feedback[bot]`, or whatever name you give it).
   - **Homepage URL:** `https://trashpanda-gamedev.github.io/Git-Raccoon/`
   - **Webhook:** untick **Active**. The relay doesn't listen for anything.
   - **Repository permissions:** **Issues: Read and write**, and **Contents: Read and write**. Leave everything else at *No access*.
   - **Where can this GitHub App be installed?** *Only on this account.*
3. **Create GitHub App.** Note the **App ID** at the top of its page.
4. Under **Private keys**, **Generate a private key**. A `.pem` file downloads. Keep it safe: it's the App's password.
5. **Install App** (left menu) › **Install** › **Only select repositories**: pick **Git-Raccoon** and **git-raccoon-feedback-images**.
6. After installing, the page's address ends in `/installations/<number>`. That number is the **installation id**.

Contents permission covers both repositories the App is installed on. The relay only ever writes screenshots to the images repository, and the key never leaves Cloudflare, but if that's too broad for you, a second App with only *Contents* on the images repository can hold that part instead.

### 2. Deploy the Worker

From this folder (`feedback-relay/`), with Node installed:

1. Put the two ids in `wrangler.toml`: `APP_ID = "…"` and `INSTALLATION_ID = "…"`.
2. Sign in to Cloudflare: `npx wrangler@3 login` (a browser window asks you to allow it).
3. Give it the private key, as a secret: `npx wrangler@3 secret put PRIVATE_KEY < path/to/the-key.pem`. Paste the file as it is; the relay takes GitHub's format.
4. Deploy: `npx wrangler@3 deploy`. It prints the relay's address, like `https://git-raccoon-feedback.<your-subdomain>.workers.dev`.
5. Try it. This creates a real issue, so close it afterwards:

   ```bash
   curl -X POST https://git-raccoon-feedback.<your-subdomain>.workers.dev/issue \
     -H "Content-Type: application/json" \
     -d '{"title":"Test from the relay","body":"Please close me.","labels":["feedback"]}'
   ```

   It should answer with the issue's number and address, and the issue should be by the App.

### 3. Point the app at it

In the Git-Raccoon repository: **Settings › Secrets and variables › Actions › Variables › New repository variable**:

- **Name:** `FEEDBACK_RELAY_URL`
- **Value:** the Worker's address from step 2.4 (no `/issue` on the end)

It's a variable, not a secret: the address is in every build anyway. The next release builds it in. Until it's set, the release still works: people signed in to GitHub post as themselves, and everyone else continues on GitHub's issue page (the release warns about it).

To try a relay without a release, run the app with it in the environment: `GIT_RACCOON_FEEDBACK_RELAY=https://… cargo run`.

### 4. Retire the old tokens

Builds up to 0.5.16 posted feedback with a personal token built into the app. Once a release with the relay is out:

1. **Revoke** both tokens (`FEEDBACK_TOKEN` and `SCREENSHOT_TOKEN`) under **GitHub › Settings › Developer settings › Personal access tokens**. Anyone can pull them out of the old builds, so this is the important step.
2. Delete the two repository secrets of those names (**Settings › Secrets and variables › Actions**). Nothing uses them any more.

After that, older versions can't post feedback themselves. Their form says it wasn't sent and offers **Send it on GitHub instead**.
