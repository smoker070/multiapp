#!/usr/bin/env bash
# Build Multiapp.app (menu-bar) + designed DMG — no Xcode needed, just Command Line Tools.
# Usage: bash build.sh
set -euo pipefail
cd "$(dirname "$0")"

APPVER="0.2.0"
BUILD="build"
APP="$BUILD/Multiapp.app"
DIST="dist"
VOL="Multiapp"

# ---------------------------------------------------------------- assets
if [ ! -f assets/icon_1024.png ] || [ ! -f assets/dmg-bg.png ]; then
  echo "— generating assets (swift make-assets.swift)…"
  mkdir -p assets
  swift make-assets.swift
fi

echo "— building AppIcon.icns…"
ICONSET="$BUILD/AppIcon.iconset"
rm -rf "$BUILD"; mkdir -p "$ICONSET" "$DIST"
for s in 16 32 128 256 512; do
  sips -z $s $s assets/icon_1024.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  d=$((s*2))
  sips -z $d $d assets/icon_1024.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$BUILD/AppIcon.icns"

# ---------------------------------------------------------------- app bundle
echo "— compiling (swiftc)…"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
swiftc -O -o "$APP/Contents/MacOS/Multiapp" Multiapp.swift
cp "$BUILD/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>com.multiapp.menubar</string>
  <key>CFBundleName</key><string>Multiapp</string>
  <key>CFBundleExecutable</key><string>Multiapp</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$APPVER</string>
  <key>LSUIElement</key><true/>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST

echo "— ad-hoc signing…"
codesign --force -s - "$APP"

echo "— installing to ~/Applications/Multiapp.app…"
rm -rf "$HOME/Applications/Multiapp.app"
cp -R "$APP" "$HOME/Applications/Multiapp.app"

# ---------------------------------------------------------------- designed DMG
echo "— staging DMG…"
STAGE="$BUILD/dmg"
mkdir -p "$STAGE/.background"
cp -R "$APP" "$STAGE/"
ln -sfn /Applications "$STAGE/Applications"
cp assets/dmg-bg.png "$STAGE/.background/bg.png"
sips -s dpiHeight 144 -s dpiWidth 144 "$STAGE/.background/bg.png" >/dev/null   # retina: 1120x680px @144dpi = 560x340pt
cp "$BUILD/AppIcon.icns" "$STAGE/.VolumeIcon.icns"

echo "— creating writable DMG…"
# eject any stale Multiapp volumes first — a pre-mounted one makes the Finder
# script target the WRONG (read-only) disk → -10006 (hit this the hard way)
for v in /Volumes/"$VOL"*; do [ -d "$v" ] && hdiutil detach "$v" -force >/dev/null 2>&1 || true; done
RW="$BUILD/rw.dmg"
rm -f "$RW"
hdiutil create -volname "$VOL" -srcfolder "$STAGE" -ov -format UDRW "$RW" >/dev/null

echo "— mounting + applying Finder layout…"
MNT=$(hdiutil attach -readwrite -noverify -noautoopen "$RW" | awk -F'\t' '/\/Volumes\//{print $NF}' | tail -1)
DISKNAME="$(basename "$MNT")"   # address Finder by the ACTUAL mount name, never assume $VOL
# volume custom-icon flag (kHasCustomIcon) without deprecated SetFile
xattr -wx com.apple.FinderInfo "0000000000000000040000000000000000000000000000000000000000000000" "$MNT" 2>/dev/null || true

osascript <<OSA || echo "   (Finder layout script failed — DMG still works, just unstyled; grant Terminal→Finder automation and rebuild)"
tell application "Finder"
  tell disk "$DISKNAME"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {200, 140, 760, 508}
    set viewOpts to the icon view options of container window
    set arrangement of viewOpts to not arranged
    set icon size of viewOpts to 112
    set text size of viewOpts to 10
    -- minimal look: no baked labels. Both items keep their real names (shown when clicked);
    -- their Finder labels stay dark-on-black under the icons → near-invisible.
    set label position of viewOpts to bottom
    set background picture of viewOpts to file ".background:bg.png"
    set position of item "Multiapp.app" of container window to {140, 165}
    set position of item "Applications" of container window to {420, 165}
    close
    open
    delay 1
    close
  end tell
end tell
OSA
sync
hdiutil detach "$MNT" >/dev/null

echo "— compressing final DMG…"
rm -f "$DIST/Multiapp-$APPVER.dmg"
hdiutil convert "$RW" -format UDZO -o "$DIST/Multiapp-$APPVER.dmg" >/dev/null
rm -f "$RW"

echo
echo "done:"
echo "  app:  ~/Applications/Multiapp.app   (menu-bar icon after launch)"
echo "  dmg:  $(cd "$DIST" && pwd)/Multiapp-$APPVER.dmg"
echo
echo "note: ad-hoc signed — fine on THIS Mac. Distributing to others needs a"
echo "Developer ID + notarization (Apple Developer Program, \$99/yr)."
