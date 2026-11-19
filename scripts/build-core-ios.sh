#!/usr/bin/env bash
# Builds the Rust core for BandEarKit: static libraries of crates/bandear-ffi for iPhone, the iOS
# Simulator on Apple silicon and macOS (where `swift test` runs), assembled into
# apps/ios/BandEarKit/Frameworks/BandEarCoreFFI.xcframework, plus the Swift bindings UniFFI
# generates into apps/ios/BandEarKit/Sources/BandEarCore/. Both are build outputs that git ignores:
# run this script once after cloning and again whenever crates/ changes, before `swift test` or an
# Xcode build.
#
# Needs macOS with Xcode, Rust, and the targets:
#   rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin
set -euo pipefail

repository="$(cd "$(dirname "$0")/.." && pwd)"
kit="$repository/apps/ios/BandEarKit"
target_dir="${CARGO_TARGET_DIR:-$repository/target}"
targets=(aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin)

# No IPHONEOS_/MACOSX_DEPLOYMENT_TARGET here: rustc's defaults are older than the iOS 16 and
# macOS 13 BandEarKit supports, so the objects link without warnings, and MACOSX_DEPLOYMENT_TARGET
# would also apply to the host's proc-macro dylibs, which a recent macOS then refuses to load.

cd "$repository"

target_args=()
for target in "${targets[@]}"; do
    target_args+=(--target "$target")
done
cargo build --release --locked -p bandear-ffi "${target_args[@]}"

# Library-mode bindgen reads the exported interface from a built library's metadata, so the Swift
# always matches the code that was just compiled.
generated="$target_dir/uniffi/swift"
rm -rf "$generated"
cargo run --quiet --locked -p bandear-ffi --features cli --bin uniffi-bindgen -- \
    generate --language swift --no-format --out-dir "$generated" \
    "$target_dir/aarch64-apple-darwin/release/libbandear_ffi.dylib"

# The C module goes in a subdirectory named after it, so its module.modulemap cannot collide with
# another xcframework's in Xcode's shared include directory.
headers="$target_dir/uniffi/headers"
rm -rf "$headers"
mkdir -p "$headers/BandEarCoreFFI"
cp "$generated/BandEarCoreFFI.h" "$headers/BandEarCoreFFI/"
cp "$generated/BandEarCoreFFI.modulemap" "$headers/BandEarCoreFFI/module.modulemap"

xcframework="$kit/Frameworks/BandEarCoreFFI.xcframework"
rm -rf "$xcframework"
library_args=()
for target in "${targets[@]}"; do
    library_args+=(-library "$target_dir/$target/release/libbandear_ffi.a" -headers "$headers")
done
xcodebuild -create-xcframework "${library_args[@]}" -output "$xcframework" >/dev/null

mkdir -p "$kit/Sources/BandEarCore"
cp "$generated/BandEarCore.swift" "$kit/Sources/BandEarCore/BandEarCore.swift"

echo "Built $xcframework"
echo "Generated $kit/Sources/BandEarCore/BandEarCore.swift"
