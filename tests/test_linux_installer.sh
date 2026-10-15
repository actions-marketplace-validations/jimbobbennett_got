#!/bin/sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)"
temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/got-installer-test.XXXXXX")"
trap 'rm -rf "$temp_dir"' 0
trap 'exit 1' HUP INT TERM

fixtures="$temp_dir/fixtures"
mock_bin="$temp_dir/bin"
mkdir -p "$fixtures" "$mock_bin"

cat > "$fixtures/got" <<'EOF'
#!/bin/sh
printf 'got 0.1.0\n'
EOF
chmod 0755 "$fixtures/got"

for arch in x86_64 aarch64; do
  archive="got-linux-$arch.tar.gz"
  tar -C "$fixtures" -czf "$fixtures/$archive" got
done
(cd "$fixtures" && sha256sum got-linux-x86_64.tar.gz got-linux-aarch64.tar.gz > SHA256SUMS)

cat > "$mock_bin/uname" <<'EOF'
#!/bin/sh
case "$1" in
  -s) printf 'Linux\n' ;;
  -m) printf '%s\n' "$GOT_TEST_ARCH" ;;
  *) exit 2 ;;
esac
EOF
cat > "$mock_bin/curl" <<'EOF'
#!/bin/sh
set -eu
output=""
url=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      output="$2"
      shift 2
      ;;
    http://* | https://*)
      url="$1"
      shift
      ;;
    *)
      shift
      ;;
  esac
done
name="${url##*/}"
cp "$GOT_FIXTURE_DIR/$name" "$output"
EOF
chmod 0755 "$mock_bin/uname" "$mock_bin/curl"

if GOT_TEST_ARCH=x86_64 \
  PATH="$mock_bin" \
  /bin/sh "$root/scripts/install-linux.sh" >/dev/null 2>"$temp_dir/missing-git.log"; then
  printf 'installer succeeded without Git on PATH\n' >&2
  exit 1
fi
grep -q "git is required" "$temp_dir/missing-git.log"

for arch in x86_64 aarch64; do
  install_dir="$temp_dir/install-$arch"
  GOT_TEST_ARCH="$arch" \
    GOT_FIXTURE_DIR="$fixtures" \
    GOT_INSTALL_DIR="$install_dir" \
    PATH="$mock_bin:$PATH" \
    sh "$root/scripts/install-linux.sh"
  [ "$("$install_dir/got")" = "got 0.1.0" ]
done

printf 'tampered' >> "$fixtures/got-linux-x86_64.tar.gz"
bad_install_dir="$temp_dir/install-bad"
if GOT_TEST_ARCH=x86_64 \
  GOT_FIXTURE_DIR="$fixtures" \
  GOT_INSTALL_DIR="$bad_install_dir" \
  PATH="$mock_bin:$PATH" \
  sh "$root/scripts/install-linux.sh" >/dev/null 2>&1; then
  printf 'installer accepted an archive with an invalid checksum\n' >&2
  exit 1
fi
[ ! -e "$bad_install_dir/got" ]

printf 'Linux installer smoke tests passed\n'
