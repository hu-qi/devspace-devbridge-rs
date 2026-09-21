#!/usr/bin/env bash
set -euo pipefail

REPO="${DEVBRIDGE_REPOSITORY:-hu-qi/devspace-devbridge-rs}"
VERSION="${DEVBRIDGE_VERSION:-latest}"
INSTALL_DIR="${DEVBRIDGE_INSTALL_DIR:-$HOME/.huawei/bin}"

case "$(uname -s)" in
  Linux) os="unknown-linux-gnu" ;;
  Darwin) os="apple-darwin" ;;
  *) echo "Unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *) echo "Unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

target="${arch}-${os}"
archive="devbridge-${target}.tar.gz"
base="https://github.com/${REPO}/releases"
if [[ "${VERSION}" == "latest" ]]; then
  url="${base}/latest/download/${archive}"
  checksum_url="${url}.sha256"
else
  version="${VERSION#v}"
  tag="v${version}"
  url="${base}/download/${tag}/${archive}"
  checksum_url="${url}.sha256"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fsSL "$url" -o "$tmp/$archive"
curl -fsSL "$checksum_url" -o "$tmp/$archive.sha256"

expected="$(awk '{print $1}' "$tmp/$archive.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/$archive" | awk '{print $1}')"
else
  actual="$(shasum -a 256 "$tmp/$archive" | awk '{print $1}')"
fi
[[ "$expected" == "$actual" ]] || { echo "SHA-256 verification failed" >&2; exit 1; }

tar -xzf "$tmp/$archive" -C "$tmp"
mkdir -p "$INSTALL_DIR"
install -m 0755 "$tmp/devbridge" "$INSTALL_DIR/devbridge"

echo "Installed devbridge to $INSTALL_DIR/devbridge"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) echo "Add $INSTALL_DIR to PATH to invoke devbridge globally." ;;
esac
