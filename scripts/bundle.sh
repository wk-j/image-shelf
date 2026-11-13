#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --release --locked
APP="out/Image Shelf.app"
mkdir -p "$APP/Contents/MacOS"
cp "${CARGO_TARGET_DIR:-target}/release/image-shelf" "$APP/Contents/MacOS/image-shelf"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>image-shelf</string>
<key>CFBundleIdentifier</key><string>local.wkj.image-shelf</string>
<key>CFBundleName</key><string>Image Shelf</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.2.0</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>CFBundleVersion</key><string>2</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST

# Ad-hoc signing only; Developer ID notarization requires the owner’s Apple account.
codesign --force --deep --sign - "$APP"
