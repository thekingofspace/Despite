#!/usr/bin/env bash
set -euo pipefail
Root="$(cd "$(dirname "$0")/.." && pwd)"
Target="x86_64-pc-windows-gnu"
Out="$Root/target/windows/Despite"
cd "$Root"
rustup target add "$Target"
cargo build -p despite-app --release --target "$Target"
rm -rf "${Out:?}"
mkdir -p "$Out"
cp "$Root/target/$Target/release/despite.exe" "$Out/Despite.exe"
if [ -d "$Root/plugins" ]; then cp -r "$Root/plugins" "$Out/plugins"; fi
if [ -f "$Root/README.md" ]; then cp "$Root/README.md" "$Out/"; fi
echo "Built $Out/Despite.exe"
