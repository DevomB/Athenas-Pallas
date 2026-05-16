# Releasing a new version

## One-time setup

1. On [crates.io](https://crates.io), create an API token with publish access.
2. In this GitHub repo: **Settings → Secrets and variables → Actions**, add **`CRATES_IO_TOKEN`** with that token.

## Ship a new version

1. Bump **`version`** under **`[workspace.package]`** in the root **`Cargo.toml`** (all workspace members use this version).
2. Commit and push to **`main`** (or **`master`**), together with any code changes for that release.

If that version is **not** already on crates.io, the **Publish crates.io** workflow runs and runs `cargo publish -p athenas-pallas`.

If the version **already** exists on crates.io (for example you only changed docs or examples without bumping the number), the workflow does nothing for publish—no duplicate upload.

## Notes

- The workflow only runs when **`Cargo.toml`** or files under **`athenas-pallas/`** change. Bump the version in the root manifest so the job runs when you intend to release.
- You cannot replace an already published version on crates.io; always bump for a new release.
- After the first successful publish, [docs.rs](https://docs.rs/athenas-pallas) builds API docs for the new version automatically.
