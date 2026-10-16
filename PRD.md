# `got checkout`

**Status:** Draft
**Date:** 2026-10-14
**Product:** `got checkout`

## Summary

`got` provides a small, easy-to-use command surface for working in a directory of a large Git repository. `got checkout` creates a shallow working copy from the latest revision of a branch (or a requested revision), using sparse checkout when a repository-relative directory is supplied. This avoids asking users to learn sparse checkout, shallow clones, or partial-clone filters. Commands such as `got status`, `got add`, `got commit`, and `got push` make the normal Git workflow work naturally from that checkout.

The core behavior is technically feasible today with native Git. GitHub Actions' `actions/checkout` already supports shallow and sparse checkout. The product opportunity is a memorable, reliable interface and safe defaults—not a new commit format or a replacement for Git hosting. For routine operations, `got` calls through to the corresponding installed `git` command instead of reimplementing Git.

## Problem

Cloning a large repository can transfer substantial history and file data, even when a developer or CI job needs only one area. This increases setup time, disk usage, and CI checkout time. Git already has features that reduce this cost, but most users do not know how to combine them or handle branch/ref details safely. They need a single command with useful defaults.

## Goals

- Check out a selected directory at the latest commit of a chosen branch, or at a specified revision.
- Avoid fetching full history and avoid materializing unrelated files.
- Allow normal edits, commits, and pushes with the selected revision as the commit's parent.
- Never overwrite a remote work branch that has advanced; detect when the base branch moved and report it before publishing.
- Provide a GitHub Actions path that checks out only the requested area.
- Publish a first-party GitHub Action that uses shallow sparse checkout by default.
- Provide a documentation website with an accessible 8-bit terminal visual style.
- Provide prebuilt installation packages for macOS Apple Silicon, Windows x64/ARM64, and Linux x86_64/ARM64; do not require end users to install Rust/Cargo.
- Start with GitHub while keeping core Git operations usable with other Git remotes.
- Make the common case one command: provide a remote and optionally a repository-relative directory; default to the remote's default branch and its latest commit.
- Support the everyday edit, stage, commit, and push workflow through `got` commands that preserve standard Git behavior.

## Non-goals for the MVP

- Replace Git, GitHub, or the Git wire protocol.
- Create a new kind of commit or a folder-only commit that is incompatible with the repository's existing tree.
- Guarantee that all repository data can be accessed offline; partial clones may fetch missing objects on demand.
- Automatically merge or force-push over concurrent branch updates.
- Provide a universal pull-request API for every Git hosting provider.
- Guarantee blob-transfer savings when a server does not support Git partial-clone filters.

## Users and primary scenarios

1. **Developer in a monorepo:** runs `got checkout <remote> [<path>]`; with a path, works and commits without materializing unrelated files, or omits the path to check out the full repository shallowly.
2. **CI maintainer:** uses the first-party Action with a directory path to fetch only that area (and any explicit supporting paths) needed for a job.
3. **Repository administrator:** enables the workflow without changing the repository's commit format, hosting provider, or existing contributor workflow.
4. **New user:** follows the documentation website's quick start and copies a working CLI or Action example.

## Product decisions for the MVP

- A checkout is still a normal repository worktree rooted at the repository root. Selected files retain their repository-relative paths; the tool may make that worktree the user's focused working directory. Flattening the directory contents to a fake repository root is out of scope because it complicates preserving the rest of the repository tree in commits.
- `got checkout <remote> [<path>]` is the primary interface. It resolves the remote's default branch and checks out its latest tip unless the user supplies a branch or revision. Omitting `<path>` checks out the whole repository with shallow history.
- The destination defaults to `<repository-name>-<path-with-slashes-replaced-by-hyphens>` in the current directory (for example, `mono-apps-api`); `--directory` overrides it. If the destination already exists, the command stops without modifying it.
- The command creates a unique normal local work branch based on the resolved commit. `got status`, `got add`, `got commit`, and `got push` operate on that repository using ordinary Git objects, commits, refs, and remotes; they do not introduce a new commit or push format.
- Ship a first-party GitHub Action alongside the CLI. The action requires a sparse path, uses shallow sparse checkout by default, and follows the triggering event's ref/SHA when no ref is specified.
- Build a public documentation website for setup, CLI reference, Action usage, compatibility limits, and troubleshooting. Use an 8-bit terminal-inspired visual style without sacrificing readability or accessibility.
- Publish prebuilt binaries from GitHub Releases. Support Homebrew on macOS ARM64, WinGet on Windows x64/ARM64, and a checksum-verifying curl installer on Linux x86_64/ARM64. Cargo is a build tool only, not an end-user installation channel.
- Distribute `got` under the MIT license and generate WinGet manifests from the declared package license and included license text; submit manifests to Microsoft's community repository rather than implying they are available before acceptance.
- The default base is the current tip of a named branch. An explicit revision must be reachable from that branch.
- Local commits are ordinary Git commits whose parent is the chosen base commit.
- To avoid requiring write access to a protected base branch, the work branch is separate from the base branch. Users can push it with standard Git; pull-request creation is provider-specific and out of scope for the initial command.
- Missing or stale refs, authentication failures, and non-fast-forward pushes are reported as errors with recovery guidance. If blob filtering is unsupported, keep sparse checkouts shallow and clearly warn that unselected blobs may still be transferred; never silently fetch full history.

## CLI interface

`got` is a small front end around native Git. `got checkout` creates and configures the sparse worktree; routine repository operations call through to the installed `git` executable.

### Checkout

```text
got checkout <remote> [<path>] [--branch <branch>] [--revision <commit>] [--directory <destination>]
```

- `<remote>` is required. Optional `<path>` is a repository-relative directory; omitting it checks out the entire repository.
- If `<remote>` is missing, print usage and exit with an error before network or filesystem side effects.
- By default, resolve the remote's default branch and use its latest commit. `--branch` selects a different base branch. `--revision` pins a commit and must be reachable from the selected branch, or from the remote's default branch when `--branch` is omitted. If reachability cannot be verified without fetching full history, deepen the fetch only as needed or fail with a clear diagnostic.
- By default, create `<repository-name>` in the current directory for a full checkout, or `<repository-name>-<path-with-slashes-replaced-by-hyphens>` for a sparse checkout. `--directory` selects a different destination. Never overwrite an existing destination.
- With `<path>`, create a sparse worktree with the selected path at its original repository-relative location. Without `<path>`, materialize the full repository. Both modes create a local work branch based on the resolved commit and use shallow history.
- On success, print the base branch and full SHA, selected path, destination, work branch, and the command to enter the checkout.

### Git operations

```text
got status [git status options]
got add <pathspec>...
got commit [git commit options]
got push [git push arguments...]
```

- `got status`, `got add`, and `got commit` forward their arguments to the corresponding Git subcommand and preserve Git's normal staging, output, and error behavior.
- `got push` with arguments forwards them unchanged to `git push`. Bare `got push` pushes the current work branch to its configured remote and sets upstream tracking on the first push.
- These commands use the current worktree and Git's configured credential helpers. They do not implement separate staging, commit, authentication, or transport behavior.
- Never force-push by default. Preserve Git's exit status and report push rejection; do not silently rewrite commits or resolve divergence.
- Before a bare `got push`, compare the recorded base SHA with the current remote base-branch tip. If it moved, warn that the work branch is based on an older revision, but still push the separate work branch; do not rebase or update the base implicitly.
- If the remote work branch itself has advanced, let Git reject a non-fast-forward push and report recovery guidance. Bare `got push` never pushes to the base branch.

### Example

```sh
got checkout https://github.com/acme/mono.git apps/api
cd mono-apps-api/apps/api
got status
got add .
got commit -m "Update API"
got push
```

## Functional requirements

### Everyday Git commands

- `got status`, `got add`, and `got commit` call through to `git status`, `git add`, and `git commit`, forwarding user arguments and preserving Git's working directory, output, exit status, and errors.
- `got commit` preserves Git's normal staging semantics; it must not implicitly stage unstaged changes unless requested through a Git option such as `-a`.
- `got push <arguments...>` calls through to `git push` with the supplied arguments unchanged.
- Bare `got push` is the convenience case: push the current work branch to its configured remote, setting the upstream on first push. If the repository has no unambiguous configured remote or current branch, fail with an actionable message instead of guessing.
- Pushes never force-update a remote by default. A non-fast-forward rejection is surfaced with guidance to fetch/rebase or otherwise resolve the divergence; `got` must not rewrite commits or resolve conflicts silently.
- These commands work inside a got checkout and may be used in an ordinary Git worktree; aside from bare `got push` defaults, they are thin pass-throughs to Git rather than a second implementation of Git behavior.

### Local checkout

- Accept the remote, optional selected path, base branch or revision, and optional destination through the command above.
- Resolve and record the exact base commit SHA before checkout.
- Create a shallow checkout. If a directory is requested, materialize only that directory plus the minimum required repository metadata/files; otherwise, materialize the full repository.
- Retain enough Git metadata to inspect status, create commits, and push a branch.
- Create a work branch from the base commit and make its name, base branch, path, base SHA, and remote visible to the user.
- Record the base branch and SHA. When the base advances, warn on push but leave refresh/rebase to ordinary Git commands; never rewrite or discard local work automatically.

### Commit and publish

- Create a regular Git commit, preserving the original repository root tree and all paths not checked out.
- Push the work branch through the remote's normal Git transport, including automatic upstream setup on first `got push`.
- For bare `got push`, detect and warn if the recorded base branch has advanced. Still allow the work-branch push because it does not update the base branch.
- Report a rejected non-fast-forward push to the work branch and leave resolution to the user; never force-push by default.
- Document how to push the work branch and open a pull request with GitHub's normal tools; do not implement pull-request creation in the initial command.
- Do not imply that a successful push to a work branch updates the base branch or creates a pull request unless that action actually succeeded.

### GitHub Actions

- Publish a first-party Action, referred to here as `got checkout`, with an optional `path` input. When `path` is absent and no `additional-paths` are supplied, check out the full repository.
- Support optional `repository`, `ref`, `fetch-depth`, `directory`, `additional-paths`, and `token` inputs. Default the repository to the workflow repository. For that repository, default the ref to the triggering event's ref/SHA, consistent with `actions/checkout`; for another repository, default to its default branch.
- Default `fetch-depth` to 1. Enable sparse checkout when `path` or `additional-paths` are supplied; otherwise check out the full repository. The action must check out the selected content into a normal repository worktree and expose the resolved commit SHA and checkout directory as outputs.
- Preserve the triggering commit when a workflow runs on a specific SHA or pull request.
- Default to least-privilege, read-only credentials and do not persist credentials for later Git commands unless explicitly requested. Workflows that need to push must request the minimum required token permissions.
- Allow additional paths (for example, shared build configuration) when the selected directory depends on them.
- Test transfer size as well as materialized files. Because `actions/checkout` documents `filter` as overriding its `sparse-checkout` input, the first-party Action must use a tested implementation that combines blob filtering and sparse path selection when supported; it must not claim blob-transfer savings based only on sparse working-tree contents. If filtering is unsupported, continue with shallow sparse checkout and emit a warning that unselected blobs may be transferred.

Illustrative workflow usage:

```yaml
permissions:
  contents: read

steps:
  - uses: cloudless-ai/got@v1
    with:
      path: apps/api
```

### Documentation website

- Publish a website with a concise home/quick-start page, installation instructions, full CLI reference, GitHub Action guide, compatibility/performance notes, and troubleshooting.
- Present commands and workflow snippets in a recognizable 8-bit terminal-inspired style. Keep body text, navigation, and forms readable rather than applying pixel styling indiscriminately.
- Meet accessibility basics: keyboard navigation, visible focus, sufficient text contrast, semantic headings, and responsive layouts.
- Keep examples synchronized with the supported CLI arguments and Action inputs; every copyable quick-start example must use required inputs.

## Non-functional requirements and success measures

- Measure checkout wall-clock time and transferred bytes against a full checkout of the same revision and runner/network.
- MVP target: for a repository where the selected directory is at most 10% of checked-out blob bytes and the remote supports blob filtering, reduce transferred blob bytes by at least 70% and reduce median checkout time by at least 50% over 10 repeated runs. Report unsupported-filter cases separately and do not claim blob-transfer savings for them.
- For any successful commit, verify that its first parent is the recorded base SHA and that unselected paths in the resulting repository tree match the base.
- Never force-push by default.
- Keep the checkout compatible with ordinary Git commands and repository-hosted code review.

## Technical feasibility

### Recommended implementation

Use native Git partial clone, shallow history, and sparse checkout rather than a custom snapshot format. Configure sparse paths before the first checkout so Git does not first materialize default top-level files:

```sh
git clone --no-checkout --depth=1 --filter=blob:none --sparse --branch <branch> <remote> <destination>
git -C <destination> sparse-checkout init --cone
git -C <destination> sparse-checkout set <directory>
git -C <destination> switch -c <work-branch> <resolved-base>
```

If the server rejects or ignores `--filter=blob:none`, retry with `--no-checkout --depth=1 --sparse` and no filter, then apply the same sparse patterns before switching to the work branch. Warn that this fallback may transfer unselected blobs; never retry without `--depth=1`.

For a chosen commit rather than the branch tip, the implementation must ensure that the commit can be fetched from the remote and that the requested base SHA is recorded. Git servers differ in whether they allow fetching an arbitrary SHA directly; resolving a branch first and fetching the required revision is the safer provider-neutral strategy.

Git's sparse worktree omits unselected paths from the filesystem, while the repository's commit still represents the complete repository tree. The unchanged parts of that tree are carried forward from their existing tree objects. A depth-1 checkout retains the selected base commit but avoids downloading its full ancestry. A blobless partial clone can avoid transferring blobs that are not needed for checkout; required blobs may be fetched later on demand.

### Feasibility findings

- **Commit and push:** Feasible with ordinary Git. A local proof using a depth-1 sparse clone created a commit with the original SHA as its parent, pushed it to a bare remote, and verified that the modified path changed while an unselected path remained unchanged.
- **GitHub support:** Strong. The official `actions/checkout` action supports `fetch-depth` and `sparse-checkout`, and its documentation includes folder-scoped checkout examples. The checkout action can persist credentials for authenticated Git commands when configured to do so.
- **GitHub Actions filtering:** The existing action documents `filter` as overriding its `sparse-checkout` input. A first-party Action that promises both sparse path selection and reduced blob transfer must combine them through a tested implementation (for example, direct Git fetch with a blob filter followed by sparse checkout); simply setting both `actions/checkout` inputs is not sufficient.
- **Generic Git hosts:** Shallow and sparse checkout are broadly usable, but transfer savings from `--filter=blob:none` require server support for Git partial-clone filtering. If the filter is rejected or unsupported, continue with depth 1 and sparse checkout, and clearly warn that unselected blobs may still be transferred. Do not silently fetch full history.
- **Exact folder-only view:** Sparse checkout reduces materialized paths but does not necessarily mean literally only that directory is present. Cone-mode sparse checkout includes files at the repository root and files in parent directories along the selected path. Non-cone patterns can narrow the view but have different performance characteristics. The MVP should document this and expose an exact-path mode only after validating its cost.
- **Offline use:** Partial clones are intentionally incomplete; accessing an omitted object can require connectivity to a promisor remote. The tool must make this visible and allow users to prefetch or hydrate data where offline work is needed.
- **Repository features:** Git LFS, submodules, generated files, build scripts, and shared root configuration can require extra paths or separate fetches. They need explicit support/configuration and should not be assumed to work merely because the target directory was checked out.
- **Alternatives:** GitHub's Contents API can download files without Git metadata, but that is a poor default for edit-and-push workflows. The Git Data API could construct trees and commits, but it adds provider-specific complexity and is unnecessary for the MVP.

### Important limits

- Checkout cost is not proportional only to the selected directory: Git must resolve commits and tree entries, and very large trees can still impose network and processing costs.
- Shallow history means local commands that traverse older ancestry may lack the required commits until history is fetched.
- A branch can advance during work. `got push` warns if the base moved but still pushes only the separate work branch; if that work branch itself moved remotely, normal Git rejects the non-fast-forward update.
- A sparse worktree represents a full repository commit. It does not create an isolated, independently pushable repository rooted at the selected folder.

## MVP scope and sequence

1. **One-command checkout:** implement `got checkout <remote> [<path>]` with default-branch resolution, a safe destination, shallow history, sparse checkout when a path is supplied, and a ready-to-use work branch.
2. **Everyday Git commands:** implement `got status`, `got add`, `got commit`, and `got push` as thin, predictable Git-compatible commands; verify staging, commit parentage, first-push upstream setup, authentication, and non-fast-forward handling.
3. **Explicit base selection:** add branch, exact revision, and destination options; show the resolved base SHA and next steps.
4. **First-party GitHub Action:** publish an Action that requires a sparse path, defaults to shallow sparse checkout, follows the triggering ref/SHA, and measures transfer savings. Document token permissions and extra paths.
5. **Documentation website:** publish the accessible 8-bit terminal-styled website with quick start, CLI reference, Action guide, troubleshooting, and compatibility notes.
6. **Installable releases:** build platform archives on GitHub-hosted runners, publish checksums and package metadata, and submit generated WinGet manifests to the community source.
7. **Compatibility and performance:** test GitHub.com and representative non-GitHub remotes, plus LFS, submodule, branch movement, filter-unavailable cases; compare full, shallow-sparse, and shallow-sparse-partial clones.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| A remote ignores or rejects partial-clone filters | Capability-check; state whether blobs may be downloaded broadly; do not silently claim partial transfer savings. |
| Unselected files are needed by build/test tooling | Allow explicit additional sparse paths and document common repository-root dependencies. |
| Base branch advances before publish | Compare with the recorded base SHA and warn before pushing the separate work branch; leave refresh/rebase to the user. |
| Sparse index or sparse paths surprise Git commands | Start with standard sparse checkout; test supported Git versions and surface hydration behavior. |
| The Action checks out only sparse working-tree files but still transfers all blobs | Measure network/object transfer, combine blob filtering with sparse checkout in the Action implementation, and report unsupported-filter behavior honestly. |
| Users mistake this for a folder-rooted independent repository | Describe it as a sparse worktree in UX and documentation. |
| CI tokens are overprivileged | Default to read-only checkout; require explicit least-privilege write permissions for publishing. |
| Terminal-inspired styling harms readability or accessibility | Use the 8-bit look as visual framing; retain accessible contrast, semantic structure, keyboard support, and responsive layouts. |

## Open questions for product validation

- Is the desired local interface a normal repository-root worktree with only selected paths present, or a flattened directory presented as the root? The MVP chooses the former to preserve native Git semantics.
- Which Git versions and non-GitHub hosting providers are required for initial support?

## References

- Git partial clone: https://git-scm.com/docs/partial-clone
- `git clone` options including `--sparse` and `--filter`: https://git-scm.com/docs/git-clone
- `git sparse-checkout` and cone-mode behavior: https://git-scm.com/docs/git-sparse-checkout
- GitHub Actions checkout options and sparse-checkout examples: https://github.com/actions/checkout
