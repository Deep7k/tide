# CI/CD

Two GitHub Actions workflows in `.github/workflows/`, both on `windows-latest`.

| Workflow | Runs on | Does |
| --- | --- | --- |
| `ci.yml` (**CI**) | every push to `master`, every pull request | format check, clippy, tests |
| `release.yml` (**Release**) | pushing a `v*` tag; or by hand | tag check, tests, build, package, publish a GitHub Release |

**Why Windows runners:** tide calls Windows-only APIs (WASAPI, PDH, DXGI, D3DKMT,
SMTC, IP Helper), so the tests can't run on Linux. Ubuntu could cross-compile the
`.exe` with `cargo-xwin`, but a release must pass the tests first, so building on
the same Windows runner keeps it to one job and produces the binary exactly the
way it's built locally (MSVC). The repo is public, so Actions minutes are free.

## Shared setup

Both workflows start the same way:

1. `actions/checkout@v7`.
2. **Toolchain:** `cargo --version`. rustup is preinstalled on the runner; the first
   cargo call installs the Rust version and components pinned in
   `rust-toolchain.toml` (1.99.0 with rustfmt and clippy). Bumping the pin there
   updates CI too.
3. `Swatinem/rust-cache@v2` caches `~/.cargo` and `target/` between runs, keyed on
   `Cargo.lock` and the toolchain. A cached release build takes about 2 minutes.

`.cargo/config.toml` (static C runtime) and the release profile in `Cargo.toml`
(LTO, stripped, `panic = "abort"`) apply on the runner as they do locally.

## CI (`ci.yml`)

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Any rustfmt difference, any clippy warning or any failing test (including the
snapshot tests) fails the run. Run the same three commands locally before pushing.
A newer push to the same branch cancels the run in progress (`concurrency`).

## Release (`release.yml`)

Triggered by pushing a tag matching `v*`. Steps:

1. **Tag matches `Cargo.toml`.** `cargo metadata` reads the package version; the run
   fails unless the tag is exactly `v<version>`. This keeps the tag, the release
   name and `tide --version` in sync.
2. **Test:** `cargo test`. A release never ships with failing tests.
3. **Build:** `cargo build --release`.
4. **Package:** `dist/tide-<tag>-windows-x64.zip` containing `tide.exe`,
   `README.md` and `LICENSE`, plus `dist/tide-<tag>-windows-x64.zip.sha256` in `sha256sum` format
   (`<hash>  <file>`).
5. **Publish:** `gh release create <tag> dist/* --title "tide <version>"
   --generate-notes --verify-tag`, authenticated with the workflow's own
   `GITHUB_TOKEN`. Tags containing `-` (e.g. `v0.3.0-rc.1`) are published as
   pre-releases. If the release already exists (a re-run), the files are replaced
   with `gh release upload --clobber` instead.

The workflow only needs `permissions: contents: write`; there are no secrets to
configure.

### Cutting a release

```powershell
# 1. bump the version
#    Cargo.toml: version = "0.3.0"
cargo build            # updates Cargo.lock
cargo test
git add Cargo.toml Cargo.lock
git commit -m "Release 0.3.0"

# 2. tag and push both
git tag -a v0.3.0 -m "tide 0.3.0"
git push origin master v0.3.0

# 3. follow it
gh run watch $(gh run list --workflow release.yml --limit 1 --json databaseId --jq '.[0].databaseId')
gh release view v0.3.0
```

The release appears at `https://github.com/Deep7k/tide/releases`.

### Dry runs and manual runs

The Release workflow also has a manual trigger (Actions → Release → Run workflow,
or `gh`) for a tag that already exists:

| Input | |
| --- | --- |
| `tag` | the existing tag to check out and build, e.g. `v0.2.0` |
| `publish` | off (default): **dry run** — the zip and checksum are uploaded as a workflow artifact and nothing is released. On: create the release, or replace the files of an existing one |

```powershell
gh workflow run release.yml -f tag=v0.2.0 -f publish=false
gh run download <run-id> -D dist-test      # fetch the artifact
```

The workflow file comes from `master`, but the code is built from the tag, so a
dry run of an old tag tests today's pipeline against that release's code.

### Release notes

`--generate-notes` lists pull requests merged since the previous tag. Changes
committed straight to `master` don't appear, so a release without PRs gets only a
"Full Changelog" compare link. Edit the notes on the release page (or
`gh release edit <tag> --notes-file notes.md`) to describe the release.

## History

| Tag | Published | Notes |
| --- | --- | --- |
| `v0.1.0` | no | Tag only; predates the workflows and `--version` |
| `v0.2.0` | yes | First published release |

## Troubleshooting

| Symptom | Cause / fix |
| --- | --- |
| Release run fails at "Tag matches Cargo.toml" | The tag and `version` differ. Delete the tag (`git push origin :refs/tags/vX.Y.Z` and `git tag -d vX.Y.Z`), fix the version, commit, tag again |
| CI fails at Format | Run `cargo fmt` and commit |
| CI fails at Clippy | Fix the warnings; CI treats them as errors (`-D warnings`) |
| Snapshot test fails in CI only | Check `snapshots/.gitattributes` still marks `*.txt -text`; without it, line-ending conversion changes the files on Windows runners |
| Run fails to create the release | The repo's Actions settings must allow workflows read and write permission, or at least the `contents: write` the workflow requests |
| Node.js deprecation annotations | Bump the action to its latest major version (check its release notes for input changes) |

## Not covered (yet)

- **Code signing.** The `.exe` is unsigned, so Windows SmartScreen may warn on first
  run. Signing needs a certificate stored as a repository secret and a signing step
  before packaging.
- **Package managers** (winget, Scoop). Both can point at the release zip and its
  SHA-256.
