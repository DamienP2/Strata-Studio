#!/usr/bin/env bash
# Construit le paquet Ubuntu de Strata Studio dans dist/.
#   packaging/build-deb.sh            (version du Cargo.toml)
#   packaging/build-deb.sh 0.1.0
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
VERSION="${VERSION#v}"
ARCH="$(dpkg --print-architecture)"
NAME="strata-studio_${VERSION}_${ARCH}"
ROOT="target/deb/${NAME}"

cargo build --release --locked -p strata-studio-server -p strata-studio-bench

rm -rf "$ROOT"
install -d "$ROOT/DEBIAN" "$ROOT/usr/bin" "$ROOT/usr/lib/systemd/system" \
    "$ROOT/etc/strata-studio" "$ROOT/usr/share/doc/strata-studio"
install -m 0755 target/release/strata-studio target/release/banc-encodage "$ROOT/usr/bin/"
install -m 0644 packaging/deb/strata-studio.service "$ROOT/usr/lib/systemd/system/"
install -m 0640 packaging/deb/strata-studio.env "$ROOT/etc/strata-studio/"
install -m 0644 README.md docs/FONCTIONNALITES.md "$ROOT/usr/share/doc/strata-studio/"
install -m 0755 packaging/deb/postinst packaging/deb/prerm packaging/deb/postrm "$ROOT/DEBIAN/"
echo "/etc/strata-studio/strata-studio.env" > "$ROOT/DEBIAN/conffiles"

GLIBC="$(ldd --version | head -1 | grep -o '[0-9]\+\.[0-9]\+$')"
SIZE="$(du -sk "$ROOT" | cut -f1)"
cat > "$ROOT/DEBIAN/control" <<CONTROL
Package: strata-studio
Version: ${VERSION}
Section: video
Priority: optional
Architecture: ${ARCH}
Depends: libc6 (>= ${GLIBC}), ffmpeg
Recommends: cifs-utils
Installed-Size: ${SIZE}
Maintainer: DamienP2 <DamienP2@users.noreply.github.com>
Homepage: https://github.com/DamienP2/Strata-Studio
Description: Strata Studio, la réalisation automatique d'un studio
 Une IA locale réalise un podcast ou une radio filmée : elle commute
 le mélangeur ATEM ou vMix, cadre les caméras PTZ, écoute les micros
 Dante en AES67, enregistre les canaux en 4:2:2 et prépare le 9:16.
CONTROL

mkdir -p dist
dpkg-deb --root-owner-group -Zxz --build "$ROOT" "dist/${NAME}.deb"
# La configuration reste lisible par le service seul.
tar -C "$ROOT" -czf "dist/strata-studio-${VERSION}-linux-${ARCH}.tar.gz" \
    usr/bin usr/lib/systemd/system etc/strata-studio
install -m 0755 packaging/install.sh dist/install.sh
(cd dist && sha256sum "${NAME}.deb" "strata-studio-${VERSION}-linux-${ARCH}.tar.gz" install.sh > SHA256SUMS)
echo "dist/${NAME}.deb"
