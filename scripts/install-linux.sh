#!/usr/bin/env bash
set -euo pipefail
Root="$(cd "$(dirname "$0")/.." && pwd)"
Prefix="${PREFIX:-$HOME/.local}"
DataHome="${XDG_DATA_HOME:-$HOME/.local/share}"
cd "$Root"
cargo build -p despite-app --release
install -Dm755 "$Root/target/release/despite" "$Prefix/bin/despite"
if [ -f "$Root/platform/linux/despite.desktop" ]; then
    install -Dm644 "$Root/platform/linux/despite.desktop" "$Prefix/share/applications/despite.desktop"
fi
if [ -d "$Root/platform/linux/hicolor" ]; then
    for Icon in "$Root"/platform/linux/hicolor/*/apps/despite.png; do
        Size="$(basename "$(dirname "$(dirname "$Icon")")")"
        install -Dm644 "$Icon" "$Prefix/share/icons/hicolor/$Size/apps/despite.png"
    done
fi
if [ -f "$Root/platform/linux/despite-mime.xml" ]; then
    install -Dm644 "$Root/platform/linux/despite-mime.xml" "$Prefix/share/mime/packages/despite.xml"
    update-mime-database "$Prefix/share/mime" 2>/dev/null || true
fi
update-desktop-database "$Prefix/share/applications" 2>/dev/null || true
if [ -d "$Root/plugins" ]; then
    mkdir -p "$DataHome/Despite/plugins"
    cp -rn "$Root"/plugins/* "$DataHome/Despite/plugins/" 2>/dev/null || true
fi
echo "Installed Despite to $Prefix"
