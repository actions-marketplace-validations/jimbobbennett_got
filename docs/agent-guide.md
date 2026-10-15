# Using got with coding agents

This guide is for agents making changes in a large repository through `got`.
For instructions on changing this project itself, see [`AGENTS.md`](../AGENTS.md).

## Start with the smallest useful checkout

For a focused sparse checkout, provide the repository remote and the
repository-relative directory:

```sh
got checkout https://github.com/acme/mono.git apps/api
cd mono-apps-api/apps/api
got status
```

The remote may also use GitHub shorthand, such as `acme/mono`. The default
checkout is based on the current tip of the remote's default branch. Use
`--branch`, `--revision`, and `--directory` when the task specifies a different
base or destination. A requested revision must be reachable from the selected
branch.

If the task needs the whole repository, omit the directory:

```sh
got checkout https://github.com/acme/mono.git
cd mono
```

This still uses shallow history, but checks out all tracked files rather than
using sparse checkout.

Use `got --help`, `got checkout --help`, or `got help push` for command help.
The actual `got` command help is the authority if this guide and a binary
version differ.

## Work and commit safely

1. Inspect `got status` before editing.
2. Stay within the requested repository paths. Sparse checkout is a working
   tree optimization, not a separate folder-only repository.
3. If a build or test needs another directory, add it to the sparse view with
   native Git, for example `git sparse-checkout add shared/build`. Do not
   disable sparse checkout just to work around a missing dependency.
4. Review the diff and stage the intended files explicitly with `got add
   <pathspec>`. This is safer than staging every visible root or parent file.
5. Commit with `got commit -m "..."`.
6. Use `got push` to publish the separate work branch. Report the branch and
   whether the push succeeded; a push does not update the base branch or create
   a pull request.

Commits are ordinary Git commits whose first parent is the selected base
revision. The repository tree still contains paths that are not present in the
working tree, so a commit can affect more than the selected directory if other
visible paths are changed and staged.

## Understand sparse and shallow behavior

- Cone-mode sparse checkout includes repository-root files and files in parent
  directories required for the selected path.
- `--filter=blob:none` can avoid transferring unselected file contents only
  when the Git server supports partial-clone filtering. `got` warns when the
  remote ignores or rejects filtering; sparse checkouts remain shallow and
  sparse, but the server may still transfer unselected blobs.
- Partial clones may need network access to fetch omitted Git objects later.
- Shallow history limits history-dependent commands. An explicit older
  revision may cause `got` to deepen history only as needed to verify that it
  is reachable from the selected base branch.
- Git LFS, submodules, generated files, and shared build configuration may
  need explicit setup or additional sparse paths.

## Push behavior

Bare `got push` selects the configured upstream or the checkout's remote and
sets upstream tracking on the first push. If the base branch moved, `got`
warns but still pushes only the work branch. Do not respond to that warning by
force-pushing or silently rebasing. If Git rejects a non-fast-forward push,
report the rejection and ask for or follow the repository's normal divergence
resolution workflow.

Passing arguments, as in `got push origin HEAD`, delegates directly to native
Git and does not apply the bare-push convenience checks.

## GitHub Actions

The Action accepts an optional `path`; omit it to check out the whole
repository. It checks out one commit by default and provides `sha` and
`directory` outputs. Grant `contents: read` for checkout;
request write permissions only when a workflow actually publishes changes.
Use `additional-paths` for explicit shared dependencies. The Action does not
persist credentials for later Git commands.
