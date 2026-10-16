#!/bin/sh
set -eu

fail() {
  printf 'got installer error: %s\n' "$*" >&2
  exit 1
}

case "$(uname -s)" in
  Linux) ;;
  *) fail "this installer supports Linux only" ;;
esac

case "$(uname -m)" in
  x86_64 | amd64) asset="got-linux-x86_64.tar.gz" ;;
  aarch64 | arm64) asset="got-linux-aarch64.tar.gz" ;;
  *) fail "unsupported Linux architecture: $(uname -m)" ;;
esac

command -v git >/dev/null 2>&1 || fail "git is required; install it with your Linux distribution's package manager"
command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v tar >/dev/null 2>&1 || fail "tar is required"

version="${GOT_VERSION:-latest}"
case "$version" in
  latest) base_url="https://github.com/cloudless-ai/got/releases/latest/download" ;;
  *[!A-Za-z0-9._-]* | "") fail "GOT_VERSION must be a release version such as 0.1.0" ;;
  v*) base_url="https://github.com/cloudless-ai/got/releases/download/$version" ;;
  *) base_url="https://github.com/cloudless-ai/got/releases/download/v$version" ;;
esac

install_dir="${GOT_INSTALL_DIR:-${HOME:?HOME must be set}/.local/bin}"
temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/got-install.XXXXXX")" || fail "could not create temporary directory"
trap 'rm -rf "$temp_dir"' 0
trap 'exit 1' HUP INT TERM

curl --fail --location --silent --show-error "$base_url/$asset" -o "$temp_dir/$asset" ||
  fail "could not download $asset from $base_url"
curl --fail --location --silent --show-error "$base_url/SHA256SUMS" -o "$temp_dir/SHA256SUMS" ||
  fail "could not download release checksums"

expected="$(awk -v name="$asset" '$2 == name { print $1; exit }' "$temp_dir/SHA256SUMS")"
[ -n "$expected" ] || fail "no checksum found for $asset"

if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$temp_dir/$asset" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$temp_dir/$asset" | awk '{ print $1 }')"
else
  fail "sha256sum or shasum is required to verify the release"
fi
[ "$actual" = "$expected" ] || fail "checksum verification failed for $asset"

tar -xzf "$temp_dir/$asset" -C "$temp_dir" || fail "could not extract $asset"
[ -f "$temp_dir/got" ] || fail "release archive does not contain the got executable"
mkdir -p "$install_dir" || fail "could not create install directory '$install_dir'"
install -m 0755 "$temp_dir/got" "$install_dir/got" || fail "could not install got to '$install_dir'"

printf 'Installed got to %s/got\n' "$install_dir"
case ":${PATH:-}:" in
  *":$install_dir:"*) ;;
  *) printf 'Add %s to PATH to run got from any directory.\n' "$install_dir" ;;
esac
