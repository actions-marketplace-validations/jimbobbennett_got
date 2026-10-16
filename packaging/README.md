# Release packaging

End users install prebuilt release binaries; Cargo is used by the release
workflow to compile the Rust application, not as an installation method.

## Release targets

| Channel | Supported target | Release artifact |
|---|---|---|
| Homebrew | macOS Apple Silicon (ARM64) | `got-darwin-aarch64.tar.gz` and generated `got.rb` |
| WinGet (coming soon) | Windows x64 and ARM64 | `got-windows-x86_64.zip`, `got-windows-aarch64.zip`, and generated manifests |
| Linux installer | x86_64 and ARM64 | `got-linux-x86_64.tar.gz`, `got-linux-aarch64.tar.gz`, and `install.sh` |

## Creating a release

1. Use [Conventional Commit](https://www.conventionalcommits.org/) messages
   such as `feat: add checkout option` or `fix: handle missing Git`. Release
   Please opens a release PR that updates `Cargo.toml`, `Cargo.lock`, and the
   changelog. The CLI reports the Cargo package version with `got --version`.
2. Review and merge the release PR. Release Please creates the version tag and
   GitHub Release; `.github/workflows/release-please.yml` then dispatches the
   packaging workflow for that tag.
3. `.github/workflows/release.yml` builds each target, creates archives and
   checksums, generates the Homebrew formula, and attaches them to the GitHub
   Release. It also updates the Action version in `README.md` and
   `docs/index.html` to the exact release tag and moves the floating major
   Action tag (for example, `v0`) to the latest stable release. Documentation
   deployment is handled by Cloudflare Workers Builds.
   To retry or repair an existing release, manually dispatch
   `release.yml` from `main` and set `release_tag` to its version tag.
4. The package declares the MIT license in `Cargo.toml` and includes the
   corresponding `LICENSE` file. The release workflow generates the three
   WinGet manifest files and attaches them to each release. Submit those files
   to [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) to make
   `winget install jimbobbennett.got` available in the community source. Until
   the manifests are accepted, Windows users can download the architecture-
   specific ZIP files directly.
5. To publish Homebrew updates automatically, create a public
   `jimbobbennett/homebrew-tap` repository, then add a fine-grained personal
   access token as the `HOMEBREW_TAP_TOKEN` Actions secret in
   `jimbobbennett/got`. Grant that token `Contents: read and write` access only
   to `homebrew-tap`. Each release creates or updates `Formula/got.rb` in the
   tap (including the initial commit if the repository is empty). If the
   secret is absent, releases still publish normally and the workflow warns
   that the tap was not updated.

## Installing

`got` requires Git 2.25 or later. Homebrew installs Git as a package
dependency; WinGet will do so once available. The Linux installer checks that
Git is available but does not install it; users installing the Windows ZIP
directly must also install Git.

### macOS Apple Silicon

After the tap repository and `HOMEBREW_TAP_TOKEN` are configured and a release
has populated the formula, install with:

```sh
brew install jimbobbennett/tap/got
```

This package is intentionally ARM64-only.

### Windows x64 and ARM64 (WinGet coming soon)

WinGet availability is coming soon. After the generated manifest is accepted
into the WinGet community source, users can install with:

```powershell
winget install jimbobbennett.got
```

Until then, Windows users can download the architecture-specific ZIP files
from GitHub Releases. The release also contains versioned manifests for
submission.

For the initial registration, wait for a tagged release, download all three
`jimbobbennett.got*.yaml` manifest assets into one directory, then validate and
submit the version manifest from Windows:

```powershell
winget validate --manifest .\manifests
wingetcreate submit --prtitle "Add got 0.1.0" .\manifests\jimbobbennett.got.yaml
```

Install WingetCreate with `winget install wingetcreate` if needed. It can open
the GitHub sign-in flow when submitting; avoid passing a token directly on the
command line. Keep each PR to one package version and only its three manifest
files. After Microsoft accepts and merges the PR, users can install with
`winget install jimbobbennett.got`. Repeat the process for each release.

### Linux x86_64 and ARM64

The installer downloads the matching archive and verifies it against
`SHA256SUMS` before installing to `~/.local/bin`:

```sh
curl -fsSL https://github.com/jimbobbennett/got/releases/latest/download/install.sh | sh
```

Set `GOT_INSTALL_DIR` to choose another destination or `GOT_VERSION` to pin a
release. The installer requires Git, `curl`, `tar`, and `sha256sum` or
`shasum`; it fails with an installation hint if Git is missing.
