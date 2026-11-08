#!/bin/sh
# Installs qsr, the Quasar command-line client, from the latest GitHub release.
#
#   curl -sSL https://raw.githubusercontent.com/AymericChaverot/quasar-cli/main/install.sh | sh
#
# QSR_VERSION=0.1.0  installs that release instead of the latest.
# QSR_INSTALL_DIR    is where the binary goes (default: ~/.local/bin).
set -eu

repo="AymericChaverot/quasar-cli"
dir="${QSR_INSTALL_DIR:-$HOME/.local/bin}"

fail() { echo "install.sh: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || fail "$1 is required"; }
need curl
need tar

case "$(uname -s)" in
  Linux) os="unknown-linux-musl" ;;
  Darwin) os="apple-darwin" ;;
  *) fail "no prebuilt binary for $(uname -s); see the README to build from source" ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch="x86_64" ;;
  aarch64 | arm64) arch="aarch64" ;;
  *) fail "no prebuilt binary for $(uname -m); see the README to build from source" ;;
esac
target="$arch-$os"

if [ -n "${QSR_VERSION:-}" ]; then
  version="${QSR_VERSION#v}"
else
  # The latest release's page redirects to its tag, which spares the API and
  # its rate limit.
  latest=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest") ||
    fail "could not find a published release of $repo"
  version="${latest##*/v}"
  [ "$version" != "$latest" ] || fail "no release published yet"
fi

name="qsr-$version-$target"
base="https://github.com/$repo/releases/download/v$version"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "Downloading qsr $version for $target..."
curl -fsSL -o "$tmp/$name.tar.gz" "$base/$name.tar.gz" || fail "could not download $base/$name.tar.gz"
curl -fsSL -o "$tmp/SHA256SUMS" "$base/SHA256SUMS" || fail "could not download the checksums"

expected=$(grep " $name.tar.gz\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)
[ -n "$expected" ] || fail "$name.tar.gz is not in the release's checksums"
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$tmp/$name.tar.gz" | cut -d' ' -f1)
else
  actual=$(shasum -a 256 "$tmp/$name.tar.gz" | cut -d' ' -f1)
fi
[ "$expected" = "$actual" ] || fail "checksum mismatch for $name.tar.gz; nothing was installed"

tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
mkdir -p "$dir"
# Moved into place rather than copied over, so a running qsr is not
# overwritten while it runs.
cp "$tmp/$name/qsr" "$dir/qsr.new"
chmod 755 "$dir/qsr.new"
mv -f "$dir/qsr.new" "$dir/qsr"

echo "Installed qsr $version to $dir/qsr"
case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "Add $dir to your PATH to run it as qsr." ;;
esac
