# Releasing a new version

This repo publishes **`athenas-pallas`** from GitHub Actions using **crates.io trusted publishing** (short-lived tokens via OIDC). You still do **one** classic publish (or a one-off CI secret) so the crate exists and you can attach a trusted publisher to it.

Official reference: [crates.io trusted publishing](https://crates.io/docs/trusted-publishing) and [`rust-lang/crates-io-auth-action`](https://github.com/rust-lang/crates-io-auth-action).

---

## Part A — From zero to first publish

### 1. crates.io account

1. Sign up at [crates.io](https://crates.io) (e.g. with GitHub).
2. **Verify your email**; publishing is blocked until you do.

### 2. Confirm the crate name

The crate name is **`athenas-pallas`**. Search on crates.io; if it is taken, change `[package] name` in `athenas-pallas/Cargo.toml` **before** the first successful publish.

### 3. Create a normal API token (first publish only)

Trusted publishing **cannot** create the crate on its own until crates.io trusts your workflow. For the **first** version, use a classic token:

1. crates.io → **Account Settings** → **API Tokens** → create a token that can **publish**.
2. On your machine (workspace root):

   ```bash
   cargo login
   cargo publish -p athenas-pallas
   ```

   Alternatively, add **`CRATES_IO_TOKEN`** to GitHub Actions secrets, temporarily change the workflow to set `CARGO_REGISTRY_TOKEN: ${{ secrets.CRATES_IO_TOKEN }}` for one run, merge, then revert to OIDC (local `cargo publish` is simpler).

After this, the crate exists under your account.

---

## Part B — Trusted publishing (no long-lived CI token)

### 4. Register the trusted publisher on crates.io

1. Open **your crate** on crates.io → **Settings** (or the crate’s admin UI for trusted publishing — follow the current crates.io UI labels).
2. Under **Trusted publishing** (wording may vary), add a **GitHub Actions** publisher with values that **exactly match** what will run in CI, for example:

   - **Repository:** `DevomB/Athenas-Pallas` (case-sensitive as GitHub shows it).
   - **Workflow file:** `publish-crates-io.yml` (the file in `.github/workflows/` in this repo).
   - **GitHub environment (optional):** leave empty unless you configure the same name on GitHub (see below). If you set an environment on crates.io, the workflow job must declare `environment: <that-name>` so the OIDC claims match.

3. Save. crates.io will only mint short-lived publish tokens when that workflow runs in that repo.

### 5. GitHub Actions (already in this repo)

The **Publish crates.io** workflow:

- Runs on **push** to **`main` / `master`** when **`Cargo.toml`** or **`athenas-pallas/**`** changes.
- Skips upload if that **version** is already on crates.io.
- Uses **`permissions: id-token: write`** and **`rust-lang/crates-io-auth-action@v1`** to obtain **`CARGO_REGISTRY_TOKEN`** for `cargo publish`.

You **do not** need **`CRATES_IO_TOKEN`** in GitHub for ongoing releases once trusted publishing works.

### 6. Remove the old secret (if you added one)

Delete **`CRATES_IO_TOKEN`** from **Settings → Secrets and variables → Actions** so nothing long-lived is stored for publishing.

---

## Part C — Routine releases

1. Bump **`version`** in **`[workspace.package]`** in the root **`Cargo.toml`**.
2. Push to **`main`** / **`master`** with changes under **`Cargo.toml`** or **`athenas-pallas/`** so the workflow runs.

If the new version is not on crates.io yet, Actions will publish using OIDC.

---

## Notes

- You **cannot** overwrite an existing version on crates.io; always bump for a new release.
- **docs.rs** builds docs shortly after each successful publish.
- If the auth step fails, re-check the trusted-publisher config on crates.io (repo, workflow filename, optional environment) against this repo’s workflow.
