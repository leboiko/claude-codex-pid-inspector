#!/usr/bin/env bash
# Render the AUR -bin PKGBUILD from the template. Output is printed to
# stdout; the user takes it to an AUR repository and `makepkg --printsrcinfo`
# to produce the matching .SRCINFO.
#
# Usage: render-aur-pkgbuild.sh <version> <sha256sums-file>

set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <version> <sha256sums-file>" >&2
  exit 2
fi

VERSION="$1"
CHECKSUMS="$2"

if [[ ! -f $CHECKSUMS ]]; then
  echo "error: checksums file not found: $CHECKSUMS" >&2
  exit 1
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
TEMPLATE="$SCRIPT_DIR/../packaging/aur/PKGBUILD-bin.tmpl"

if [[ ! -f $TEMPLATE ]]; then
  echo "error: template not found: $TEMPLATE" >&2
  exit 1
fi

sha_for() {
  local needle="$1"
  awk -v n="$needle" '$2 ~ n { print $1; exit }' "$CHECKSUMS"
}

BASE_URL="https://github.com/leboiko/claude-codex-pid-inspector/releases/download/v${VERSION}"

URL_X86_64_LINUX_GNU="${BASE_URL}/agentop-${VERSION}-x86_64-unknown-linux-gnu.tar.gz"
URL_AARCH64_LINUX_GNU="${BASE_URL}/agentop-${VERSION}-aarch64-unknown-linux-gnu.tar.gz"

SHA_X86_64_LINUX_GNU="$(sha_for "x86_64-unknown-linux-gnu.tar.gz")"
SHA_AARCH64_LINUX_GNU="$(sha_for "aarch64-unknown-linux-gnu.tar.gz")"

for var in SHA_X86_64_LINUX_GNU SHA_AARCH64_LINUX_GNU; do
  if [[ -z ${!var} ]]; then
    echo "error: could not find checksum for $var in $CHECKSUMS" >&2
    exit 1
  fi
done

sed \
  -e "s|{{VERSION}}|${VERSION}|g" \
  -e "s|{{URL_X86_64_LINUX_GNU}}|${URL_X86_64_LINUX_GNU}|g" \
  -e "s|{{URL_AARCH64_LINUX_GNU}}|${URL_AARCH64_LINUX_GNU}|g" \
  -e "s|{{SHA256_X86_64_LINUX_GNU}}|${SHA_X86_64_LINUX_GNU}|g" \
  -e "s|{{SHA256_AARCH64_LINUX_GNU}}|${SHA_AARCH64_LINUX_GNU}|g" \
  "$TEMPLATE"
