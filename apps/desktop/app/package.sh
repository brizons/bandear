#!/usr/bin/env bash
# Packages the release build of the desktop app for one Rust target into dist/, for the release
# workflow: a .dmg holding BandEar.app on macOS (ad-hoc signed, as the Tauri bundle was), a zip with
# BandEar.exe on Windows and a tarball on Linux. Run it after
#   cargo build --release --locked -p bandear-desktop --target <target>
# Usage: apps/desktop/app/package.sh <target>
set -euo pipefail

target="${1:?usage: package.sh <rust target>}"
root="$(cd "$(dirname "$0")/../../.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/apps/desktop/app/Cargo.toml" | head -n 1)"
built="$root/target/$target/release"
dist="$root/dist"
mkdir -p "$dist"

case "$target" in
  *-apple-darwin)
    arch="${target%%-*}"
    staging="$(mktemp -d)"
    app="$staging/BandEar.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "$built/bandear-desktop" "$app/Contents/MacOS/BandEar"
    cp "$root/apps/desktop/app/icons/icon.icns" "$app/Contents/Resources/BandEar.icns"
    # The microphone description is what macOS shows before the engine opens the audio interface;
    # without it the system ends the app at its first capture.
    cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>BandEar</string>
  <key>CFBundleDisplayName</key><string>BandEar</string>
  <key>CFBundleIdentifier</key><string>com.bandear.desktop</string>
  <key>CFBundleExecutable</key><string>BandEar</string>
  <key>CFBundleIconFile</key><string>BandEar</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>BandEar listens to your audio interface to send each musician their mix.</string>
</dict>
</plist>
PLIST
    codesign --force --deep --sign - "$app"
    hdiutil create -volname BandEar -srcfolder "$app" -ov -format UDZO \
      "$dist/BandEar_${version}_${arch}.dmg"
    rm -rf "$staging"
    ;;
  *-windows-*)
    staging="$(mktemp -d)"
    cp "$built/bandear-desktop.exe" "$staging/BandEar.exe"
    (cd "$staging" && 7z a -tzip "$dist/BandEar_${version}_x64.zip" BandEar.exe >/dev/null)
    rm -rf "$staging"
    ;;
  *-linux-*)
    staging="$(mktemp -d)"
    cp "$built/bandear-desktop" "$staging/bandear"
    tar -czf "$dist/BandEar_${version}_amd64.tar.gz" -C "$staging" bandear
    rm -rf "$staging"
    ;;
  *)
    echo "package.sh: no package for target $target" >&2
    exit 1
    ;;
esac
ls -l "$dist"
