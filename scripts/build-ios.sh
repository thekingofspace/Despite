#!/usr/bin/env bash
set -euo pipefail
Root="$(cd "$(dirname "$0")/.." && pwd)"
Target="${1:-aarch64-apple-ios-sim}"
Profile="${PROFILE:-release}"
Out="$Root/target/ios/$Target"
App="$Out/Despite.app"
ProfileArgs=()
if [ "$Profile" = "release" ]; then ProfileArgs+=(--release); fi
cd "$Root"
rustup target add "$Target"
cargo build -p despite-app --no-default-features --target "$Target" "${ProfileArgs[@]}"
rm -rf "${App:?}"
mkdir -p "$App"
cp "$Root/target/$Target/$Profile/despite" "$App/Despite"
cp "$Root/platform/ios/Info.plist" "$App/Info.plist"
Master="$(ls "$Root"/platform/ios/AppIcon.appiconset/*.png 2>/dev/null | head -1 || true)"
if [ -n "$Master" ] && command -v sips >/dev/null; then
    for Spec in "AppIcon60x60@2x:120" "AppIcon60x60@3x:180" "AppIcon76x76@2x~ipad:152" "AppIcon83.5x83.5@2x~ipad:167"; do
        sips -z "${Spec##*:}" "${Spec##*:}" "$Master" --out "$App/${Spec%%:*}.png" >/dev/null
    done
fi
if [ -n "${IOS_SIGN_IDENTITY:-}" ] && [ -n "${IOS_PROVISIONING_PROFILE:-}" ]; then
    cp "$IOS_PROVISIONING_PROFILE" "$App/embedded.mobileprovision"
    security cms -D -i "$IOS_PROVISIONING_PROFILE" > "$Out/profile.plist"
    /usr/libexec/PlistBuddy -x -c "Print :Entitlements" "$Out/profile.plist" > "$Out/entitlements.plist"
    codesign --force --sign "$IOS_SIGN_IDENTITY" --entitlements "$Out/entitlements.plist" "$App"
    echo "Signed with $IOS_SIGN_IDENTITY"
else
    codesign --force --sign - "$App"
    if [ "$Target" = "aarch64-apple-ios" ]; then
        echo "Ad-hoc signed. Re-sign it with SideStore/AltStore, or set IOS_SIGN_IDENTITY and IOS_PROVISIONING_PROFILE." >&2
    fi
fi
echo "Built $App"
if [ "${IPA:-0}" = "1" ]; then
    Stage="$Out/ipa"
    rm -rf "${Stage:?}"
    mkdir -p "$Stage/Payload"
    cp -R "$App" "$Stage/Payload/"
    rm -f "${Out:?}/Despite.ipa"
    (cd "$Stage" && zip -qry "$Out/Despite.ipa" Payload)
    echo "Built $Out/Despite.ipa"
fi
if [ "${INSTALL:-0}" = "1" ]; then
    if [ "$Target" = "aarch64-apple-ios" ]; then
        xcrun devicectl device install app --device "${IOS_DEVICE:?set IOS_DEVICE to your device id from xcrun devicectl list devices}" "$App"
    else
        xcrun simctl install booted "$App"
        xcrun simctl launch booted org.despite.app
    fi
fi
