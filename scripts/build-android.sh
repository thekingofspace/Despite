#!/usr/bin/env bash
set -euo pipefail
Root="$(cd "$(dirname "$0")/.." && pwd)"
Sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}"
export ANDROID_HOME="$Sdk"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$(ls -d "$Sdk"/ndk/* | sort -V | tail -1)}"
BuildTools="$(ls -d "$Sdk"/build-tools/* | sort -V | tail -1)"
Platform="$(ls -d "$Sdk"/platforms/android-* | sort -V | tail -1)"
Profile="${PROFILE:-release}"
Abis="${ABIS:-arm64-v8a armeabi-v7a x86_64}"
Out="$Root/target/android"
Keystore="${ANDROID_KEYSTORE:-$HOME/.android/debug.keystore}"
KeystorePass="${ANDROID_KEYSTORE_PASS:-android}"
KeyAlias="${ANDROID_KEY_ALIAS:-androiddebugkey}"
rm -rf "${Out:?}/lib" "${Out:?}/staging"
mkdir -p "$Out/lib" "$Out/staging"
TargetArgs=()
for Abi in $Abis; do TargetArgs+=(-t "$Abi"); done
ProfileArgs=()
if [ "$Profile" = "release" ]; then ProfileArgs+=(--release); fi
cd "$Root"
cargo ndk "${TargetArgs[@]}" -P 26 -o "$Out/lib" build -p despite-android "${ProfileArgs[@]}"
LinkArgs=(-o "$Out/staging/unsigned.apk" -I "$Platform/android.jar" --manifest "$Root/platform/android/AndroidManifest.xml" --auto-add-overlay)
if [ -d "$Root/platform/android/res" ]; then
    "$BuildTools/aapt2" compile --dir "$Root/platform/android/res" -o "$Out/staging/res.zip"
    LinkArgs+=(-R "$Out/staging/res.zip")
fi
"$BuildTools/aapt2" link "${LinkArgs[@]}"
python3 -I - "$Out/staging/unsigned.apk" "$Out/lib" <<'PYTHON'
import os, sys, zipfile
Apk, LibRoot = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(Apk, "a") as Archive:
    for Abi in sorted(os.listdir(LibRoot)):
        for Name in sorted(os.listdir(os.path.join(LibRoot, Abi))):
            if Name.endswith(".so"):
                Archive.write(os.path.join(LibRoot, Abi, Name), f"lib/{Abi}/{Name}", compress_type=zipfile.ZIP_STORED)
PYTHON
"$BuildTools/zipalign" -f -P 16 4 "$Out/staging/unsigned.apk" "$Out/staging/aligned.apk"
if [ ! -f "$Keystore" ]; then
    mkdir -p "$(dirname "$Keystore")"
    keytool -genkeypair -v -keystore "$Keystore" -storepass "$KeystorePass" -keypass "$KeystorePass" -alias "$KeyAlias" -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US"
fi
"$BuildTools/apksigner" sign --ks "$Keystore" --ks-pass "pass:$KeystorePass" --ks-key-alias "$KeyAlias" --out "$Out/despite.apk" "$Out/staging/aligned.apk"
"$BuildTools/apksigner" verify "$Out/despite.apk"
echo "Built $Out/despite.apk"
if [ "${INSTALL:-0}" = "1" ]; then
    "$Sdk/platform-tools/adb" install -r "$Out/despite.apk"
    "$Sdk/platform-tools/adb" shell am start -n org.despite.app/android.app.NativeActivity
fi
