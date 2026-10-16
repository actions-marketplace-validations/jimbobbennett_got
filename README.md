# got

`got` makes it faster to work with large Git repositories. Check out a shallow
copy of the whole repository or only the folders you need, then keep using
normal Git commits and pushes.

## Install

Prebuilt packages are published with tagged releases:

- **macOS:** `brew install cloudless-ai/tap/got`
- **Windows:** WinGet is coming soon; Windows ZIPs will be available from
  [GitHub Releases](https://github.com/cloudless-ai/got/releases/latest).
- **Linux:**

  ```sh
  curl -fsSL https://github.com/cloudless-ai/got/releases/latest/download/install.sh | sh
  ```

`got` requires Git 2.25 or later. Homebrew installs Git automatically; WinGet
will do so once available. Install Git separately when using a Windows ZIP or
the Linux installer. See [release packaging](packaging/README.md) for package
availability and details.

## Quick start

Check out a folder or the whole repository:

```sh
got checkout https://github.com/owner/repo.git apps/api
got checkout https://github.com/owner/repo.git
```

In the checkout, use Git as usual:

```sh
got status
got add .
got commit -m "Update API"
got push
```

`got push` publishes the work branch and sets its upstream on the first push.
It warns if the base branch moved and never rebases or force-pushes
automatically.

By default, `got` uses the remote's default branch and latest commit. Use
`--branch`, `--revision`, or `--directory` to choose a different base or
destination. Run `got --version` to check the installed version, or
`got checkout --help` for options.

## GitHub Action

The Action checks out the full repository by default. Set `path` to limit the
checkout to a folder:

```yaml
permissions:
  contents: read

steps:
  - uses: cloudless-ai/got@v0.2.1
    with:
      path: apps/api
```

See the [documentation site](https://getgot.sh) for the CLI, Action inputs, and
benchmarking details.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, tests, and pull
request guidance.

## License

MIT. See [LICENSE](LICENSE).
