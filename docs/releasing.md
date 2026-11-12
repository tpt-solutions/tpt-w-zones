# Releasing

Written for: the maintainer cutting a release.

## One-time setup (needs the repository owner)

1. **Signing key for updates.** Run `npx tauri signer generate -w tauri-signing.key` in
   `apps/editor`. Store the printed public key for the updater configuration. Keep the private key
   and its password out of the repository.
2. **GitHub secrets.** In the repository settings, add:
   - `TAURI_SIGNING_PRIVATE_KEY`: the contents of the private key file,
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password.
   Without these, the workflow still builds and attaches unsigned installers.

## Cutting a release

1. Bump `version` in the workspace `Cargo.toml`, in `apps/editor/package.json`, and in
   `apps/editor/src-tauri/tauri.conf.json`. Keep them equal.
2. Update `CHANGELOG.md` (create it on the first release).
3. Commit, then tag: `git tag v0.1.0 && git push origin v0.1.0`.
4. The `Release` workflow builds the MSI and NSIS installers and attaches them to a **draft**
   GitHub Release. The workflow fails if the tag and the crate version differ.
5. Download the installers from the draft, install them on a clean machine, and run the checks in
   `docs/testing-windows.md`. Then publish the release.

Tags containing a hyphen (for example `v0.2.0-rc1`) are marked as pre-releases.

## Not yet automated

- The in-app updater (`tauri-plugin-updater`) is not wired up. It needs the public key from the
  setup above and a `latest.json` asset, both added in a later change.
- The winget manifest needs the published installer URL and its SHA-256 hash.
