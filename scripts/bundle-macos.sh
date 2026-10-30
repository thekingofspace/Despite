#!/usr/bin/env bash
set -euo pipefail
Root="$(cd "$(dirname "$0")/.." && pwd)"
Out="$Root/target/macos"
App="$Out/Despite.app"
cd "$Root"
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build -p despite-app --release --target aarch64-apple-darwin
cargo build -p despite-app --release --target x86_64-apple-darwin
rm -rf "${App:?}"
mkdir -p "$App/Contents/MacOS" "$App/Contents/Resources"
lipo -create -output "$App/Contents/MacOS/despite" "$Root/target/aarch64-apple-darwin/release/despite" "$Root/target/x86_64-apple-darwin/release/despite"
cp "$Root/platform/macos/Info.plist" "$App/Contents/Info.plist"
if [ -f "$Root/platform/macos/despite.icns" ]; then cp "$Root/platform/macos/despite.icns" "$App/Contents/Resources/despite.icns"; fi
if [ -d "$Root/plugins" ]; then cp -R "$Root/plugins" "$App/Contents/MacOS/plugins"; fi
codesign --force --deep --sign "${MACOS_SIGN_IDENTITY:--}" "$App"
echo "Built $App"
if [ "${DMG:-0}" = "1" ]; then
    rm -f "${Out:?}/Despite.dmg"
    hdiutil create -volname Despite -srcfolder "$App" -ov -format UDZO "$Out/Despite.dmg"
    echo "Built $Out/Despite.dmg"
fi
