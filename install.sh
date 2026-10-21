#!/bin/sh
# Build and install tufctl system-wide.
#
#   ./install.sh                 # installs into /usr/local with sudo
#   PREFIX=/usr ./install.sh     # custom prefix (packaging)
#   DESTDIR=/tmp/pkg ./install.sh
#
# The binary and its polkit action are installed root-owned; a prefix inside
# $HOME is refused because polkit elevates this binary and it must not be
# user-writable.

set -eu

PREFIX=${PREFIX:-/usr/local}
DESTDIR=${DESTDIR:-}
POLICY_DIR=${POLICY_DIR:-/usr/share/polkit-1/actions}

BINDIR="$PREFIX/bin"
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

cd "$ROOT"

case "$PREFIX" in
"$HOME" | "$HOME"/*)
    echo "error: refusing to install under \$HOME ($PREFIX); use a system prefix" >&2
    exit 1
    ;;
esac

echo "==> building release binary"
cargo build --release

stage=$(mktemp)
trap 'rm -f "$stage"' EXIT

sed "s|@BINDIR@|$BINDIR|g" packaging/org.asus.tufctl.policy.in >"$stage"

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
    SUDO="sudo"
fi

echo "==> installing $BINDIR/tufctl"
$SUDO install -Dm755 "$ROOT/target/release/tufctl" "$DESTDIR$BINDIR/tufctl"

echo "==> installing $POLICY_DIR/org.asus.tufctl.policy"
$SUDO install -Dm644 "$stage" "$DESTDIR$POLICY_DIR/org.asus.tufctl.policy"

echo
echo "done. try:"
echo "  tufctl status"
echo "  tufctl kbd 2"
echo "  tufctl rgb static 0 255 255 --save"
