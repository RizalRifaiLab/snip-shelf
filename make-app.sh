#!/bin/sh
# Builds "Snip Shelf.app" and "dist/SnipShelf.dmg" next to this script.
# The app runs on both Apple-chip and Intel Macs (a "universal" app).
set -e
cd "$(dirname "$0")"

VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
export MACOSX_DEPLOYMENT_TARGET=11.0
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

APP="Snip Shelf.app"
WORK="target/package"
rm -rf "$APP" "$WORK"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$WORK"
lipo -create -output "$APP/Contents/MacOS/snip-shelf" \
  target/aarch64-apple-darwin/release/snip-shelf \
  target/x86_64-apple-darwin/release/snip-shelf

# App icon: draw it, then turn it into the macOS .icns format.
ICONSET="$WORK/AppIcon.iconset"
mkdir -p "$ICONSET"
"$APP/Contents/MacOS/snip-shelf" --write-icon "$ICONSET/icon_512x512@2x.png"
for s in 16 32 128 256 512; do
  sips -z $s $s "$ICONSET/icon_512x512@2x.png" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2))
  sips -z $d $d "$ICONSET/icon_512x512@2x.png" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Snip Shelf</string>
  <key>CFBundleDisplayName</key><string>Snip Shelf</string>
  <key>CFBundleIdentifier</key><string>local.snipshelf.app</string>
  <key>CFBundleExecutable</key><string>snip-shelf</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSScreenCaptureUsageDescription</key><string>Snip Shelf uses screen capture for the Snip and Record buttons.</string>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true
echo "Built $(pwd)/$APP"

# Disk image: open it and drag Snip Shelf onto the Applications shortcut to install.
STAGE="$WORK/dmg"
mkdir -p "$STAGE" dist
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "Snip Shelf" -srcfolder "$STAGE" -ov -format UDZO "dist/SnipShelf-$VERSION.dmg" >/dev/null
echo "Built $(pwd)/dist/SnipShelf-$VERSION.dmg"
