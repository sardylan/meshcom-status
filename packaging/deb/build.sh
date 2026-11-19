#!/bin/sh
# Build the Debian package of meshcom-status from a release binary.
#
#   packaging/deb/build.sh BINARY ARCH VERSION OUTDIR
#
#   BINARY   the stripped Linux release binary (for example from the Dockerfile's `artifact` target)
#   ARCH     Debian architecture: amd64 or arm64
#   VERSION  the semantic version (Cargo.toml), e.g. 1.2.3 or 1.2.3-rc.1
#   OUTDIR   where to write meshcom-status_<debian-version>_<arch>.deb
#
set -eu

if [ $# -ne 4 ]; then
    sed -n '2,10p' "$0" >&2
    exit 2
fi
BINARY=$1
ARCH=$2
VERSION=$3
OUTDIR=$4
NAME=meshcom-status
MAINTAINER=${DEB_MAINTAINER:-"meshcom-status developers <noreply@example.com>"}
HERE=$(dirname "$0")

case "$ARCH" in
    amd64 | arm64) ;;
    *) echo "ARCH must be amd64 or arm64, not $ARCH" >&2; exit 2 ;;
esac
DEB_VERSION="$(printf '%s' "$VERSION" | sed 's/+.*//; s/-/~/')-1"

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
ROOT="$STAGE/$NAME"
DOC="$ROOT/usr/share/doc/$NAME"

install -D -m 0755 "$BINARY" "$ROOT/usr/bin/$NAME"
install -D -m 0640 packaging/meshcom-status.toml "$ROOT/etc/$NAME/$NAME.toml"
install -D -m 0644 "$HERE/$NAME.service" "$ROOT/usr/lib/systemd/system/$NAME.service"
install -d -m 0755 "$DOC"
gzip -9n < README.md > "$DOC/README.md.gz"
install -D -m 0644 "$HERE/lintian-overrides" "$ROOT/usr/share/lintian/overrides/$NAME"

cat > "$DOC/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: $NAME

Files: *
Copyright: 2026 $NAME developers
License: GPL-3.0-or-later
 This program is free software: you can redistribute it and/or modify it under the terms of
 the GNU General Public License as published by the Free Software Foundation, either version 3
 of the License, or (at your option) any later version.
 .
 On Debian systems, the complete text of the GNU General Public License version 3 can be found
 in "/usr/share/common-licenses/GPL-3".
EOF

DATE=$(date -u -R -d "@${SOURCE_DATE_EPOCH:-$(date +%s)}")
cat <<EOF | gzip -9n > "$DOC/changelog.Debian.gz"
$NAME ($DEB_VERSION) stable; urgency=medium

  * Release $VERSION.

 -- $MAINTAINER  $DATE
EOF

GLIBC=$(objdump -T "$ROOT/usr/bin/$NAME" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -n 1)
DEPENDS="libc6 (>= $GLIBC), libgcc-s1, adduser, init-system-helpers (>= 1.52)"

SIZE=$(du -sk --apparent-size "$ROOT" | cut -f1)
install -d -m 0755 "$ROOT/DEBIAN"
cat > "$ROOT/DEBIAN/control" <<EOF
Package: $NAME
Version: $DEB_VERSION
Architecture: $ARCH
Maintainer: $MAINTAINER
Installed-Size: $SIZE
Depends: $DEPENDS
Recommends: postgresql
Section: net
Priority: optional
Description: status server for the MeshCom network
 Reads the frames a MeshCom node receives (net console or KISS) and shows
 on a live web map which nodes hear which, with paths, signal reports, a
 chat view and statistics. Frames are stored in PostgreSQL.
 .
 The service runs as the unprivileged meshcom-status user and is
 configured in /etc/meshcom-status/meshcom-status.toml.
EOF
echo "/etc/$NAME/$NAME.toml" > "$ROOT/DEBIAN/conffiles"
for script in postinst prerm postrm; do
    install -m 0755 "$HERE/$script" "$ROOT/DEBIAN/$script"
done
(cd "$ROOT" && find . -type f ! -path './DEBIAN/*' -printf '%P\0' | sort -z | xargs -0 md5sum) > "$ROOT/DEBIAN/md5sums"
chmod 0644 "$ROOT/DEBIAN/control" "$ROOT/DEBIAN/conffiles" "$ROOT/DEBIAN/md5sums"

mkdir -p "$OUTDIR"
OUT="$OUTDIR/${NAME}_${DEB_VERSION}_${ARCH}.deb"
dpkg-deb --root-owner-group -Zxz --build "$ROOT" "$OUT"
echo "$OUT"
