# Contributing

Thanks for helping improve `got`. It is a Rust CLI and GitHub Action for fast,
shallow checkouts of whole repositories or selected folders.

## Development

Install Rust stable, Cargo, Git 2.25 or later, and Python 3. From the
repository root, run the checks relevant to your change:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
bash -n action/checkout.sh
sh -n scripts/install-linux.sh
python3 -m unittest discover -s tests -p 'test_release_packaging.py'
```

On Linux, also run the installer smoke test:

```sh
sh tests/test_linux_installer.sh
```

The CLI integration tests use local Git repositories; they do not need access
to GitHub. GitHub Actions also tests the CLI and Action on Ubuntu, macOS, and
Windows.

## Pull requests

- Open an issue first for substantial behavior changes; small fixes and
  documentation improvements can go straight to a pull request.
- Describe the problem, the change, and the checks you ran.
- Keep user-facing behavior, command help, the README, and the documentation
  site in sync.
- Preserve native Git behavior. Do not silently stage changes, rebase, or
  force-push.

See [AGENTS.md](AGENTS.md) for additional repository-specific implementation
guidance.
