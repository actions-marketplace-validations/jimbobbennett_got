# Repository guidance for coding agents

## Project overview

`got` is a cross-platform Rust CLI that orchestrates native Git for shallow,
sparse checkouts. It does not define a new repository or commit format.

## Sources of truth

- `PRD.md` defines product requirements and technical decisions.
- `README.md` is the user-facing installation and quick-start guide.
- `src/main.rs` implements the CLI.
- `action.yml` and `action/checkout.sh` implement the GitHub Action.
- `docs/` contains the static documentation site and agent usage guide.
- `scripts/` and `packaging/` contain release tooling and package instructions.

Keep these surfaces consistent when changing commands, options, defaults,
Action inputs, or safety behavior.

## Implementation constraints

- Prefer the Rust standard library; do not add dependencies without a clear
  need.
- Invoke Git with argument vectors through `std::process::Command`; do not
  interpolate repository paths, refs, or user arguments into shell commands.
- Preserve native Git behavior for `status`, `add`, `commit`, and explicit
  `push` arguments. Do not silently stage files, rebase, or force-push.
- Keep checkouts shallow when blob filtering is unavailable. Sparse checkouts
  stay sparse and report the loss of transfer savings instead of silently
  fetching full history.
- Keep behavior portable across Windows, macOS, and Linux. The Action uses
  Bash and must remain compatible with the shells available on GitHub-hosted
  runners.
- Surface failures with actionable errors. Do not hide operational errors
  behind broad catches or success-shaped fallbacks.

## Validation

Run the relevant checks before handing off:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
bash -n action/checkout.sh
sh -n scripts/install-linux.sh
python3 -m unittest discover -s tests -p 'test_release_packaging.py'
sh tests/test_linux_installer.sh
```

The integration tests use local bare Git repositories. `.github/workflows/ci.yml`
also exercises the CLI and Action on Ubuntu, macOS, and Windows.

## Documentation

Update `README.md`, `docs/index.html`, `docs/agent-guide.md`, `llms.txt`, and
`packaging/README.md` when user-visible behavior or install targets change.
Keep command help in `src/main.rs` aligned with those documents.
