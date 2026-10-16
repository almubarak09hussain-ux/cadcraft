#!/usr/bin/env bash
# Build the iPad app (apps/cadcraft-ios) on macOS with Xcode and XcodeGen (`brew install xcodegen`):
#
#   dist/ios/CADCraft-simulator.app                  ad hoc signed, for the iOS Simulator
#                                                    (packaging/ios/run-simulator.sh)
#   dist/ios/cadcraft-<version>-ipad-unsigned.ipa    for an iPad; it is signed when installed
#                                                    (Sideloadly or AltStore with an Apple ID)
#   --testflight: signs the app for the team and uploads it to App Store Connect, for TestFlight
#
# Usage: packaging/ios/build.sh [--simulator] [--device] [--testflight]   (default: the first two)
#
# Env:
#   CADCRAFT_BUNDLE_ID      bundle identifier (default local.cadcraft.ipad)
#   CADCRAFT_BUILD_NUMBER   CFBundleVersion (default: the commit count); TestFlight needs it to grow
# --testflight also needs an App Store Connect API key (role Admin, for cloud-managed signing):
#   APPLE_TEAM_ID, ASC_KEY_ID, ASC_ISSUER_ID, ASC_KEY_PATH (the AuthKey_<id>.p8 file)
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/ios"
OUT="$ROOT/dist/ios"
XC="$OUT/xcode"

SIM=0
DEVICE=0
TESTFLIGHT=0
while [ $# -gt 0 ]; do
  case "$1" in
    --simulator) SIM=1; shift ;;
    --device) DEVICE=1; shift ;;
    --testflight) TESTFLIGHT=1; shift ;;
    -h | --help) sed -n '2,18p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
if [ "$SIM" = 0 ] && [ "$DEVICE" = 0 ] && [ "$TESTFLIGHT" = 0 ]; then
  SIM=1
  DEVICE=1
fi
if ! command -v xcodegen >/dev/null; then
  echo "error: xcodegen not found (brew install xcodegen)" >&2
  exit 1
fi
if [ "$TESTFLIGHT" = 1 ]; then
  for v in APPLE_TEAM_ID ASC_KEY_ID ASC_ISSUER_ID ASC_KEY_PATH; do
    if [ -z "${!v:-}" ]; then
      echo "error: --testflight needs $v" >&2
      exit 1
    fi
  done
  if [ ! -f "$ASC_KEY_PATH" ]; then
    echo "error: ASC_KEY_PATH $ASC_KEY_PATH is not a file" >&2
    exit 1
  fi
fi

# Keep in sync with deploymentTarget in project.yml.
export IPHONEOS_DEPLOYMENT_TARGET=16.0
export CADCRAFT_BUNDLE_ID="${CADCRAFT_BUNDLE_ID:-local.cadcraft.ipad}"
export CADCRAFT_SHORT_VERSION="${VERSION%%-*}"
CADCRAFT_BUILD_NUMBER="${CADCRAFT_BUILD_NUMBER:-$(git -C "$ROOT" rev-list --count HEAD 2>/dev/null || echo 1)}"
export CADCRAFT_BUILD_NUMBER

# 1. The Rust binary, for each platform asked for.
TARGETS=()
if [ "$SIM" = 1 ]; then TARGETS+=(aarch64-apple-ios-sim); fi
if [ "$DEVICE" = 1 ] || [ "$TESTFLIGHT" = 1 ]; then TARGETS+=(aarch64-apple-ios); fi
if command -v rustup >/dev/null; then rustup target add "${TARGETS[@]}"; fi
CARGO_TARGETS=()
for t in "${TARGETS[@]}"; do CARGO_TARGETS+=(--target "$t"); done
cargo build --release --locked -p cadcraft-ios "${CARGO_TARGETS[@]}"

# 2. The Xcode project, generated in dist/ios/xcode with the app icon.
rm -rf "$XC"
mkdir -p "$XC"
cp -R "$HERE/project.yml" "$HERE/Info.plist" "$HERE/PrivacyInfo.xcprivacy" "$HERE/Assets.xcassets" "$XC/"
cp "$ROOT/assets/app-icon/cadcraft-ios-1024.png" "$XC/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png"
xcodegen generate --spec "$XC/project.yml" --project "$XC"

xcode_build() {
  local sdk="$1"
  shift
  xcodebuild -project "$XC/CADCraft.xcodeproj" -target CADCraft -configuration Release -sdk "$sdk" \
    SYMROOT="$OUT/build" "$@" build
}

# 3. The app bundles.
if [ "$SIM" = 1 ]; then
  xcode_build iphonesimulator
  rm -rf "$OUT/CADCraft-simulator.app"
  cp -R "$OUT/build/Release-iphonesimulator/CADCraft.app" "$OUT/CADCraft-simulator.app"
  echo "Simulator app: $OUT/CADCraft-simulator.app"
fi
if [ "$DEVICE" = 1 ]; then
  xcode_build iphoneos CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO
  IPA="$OUT/cadcraft-$VERSION-ipad-unsigned.ipa"
  rm -rf "$OUT/ipa" "$IPA"
  mkdir -p "$OUT/ipa/Payload"
  cp -R "$OUT/build/Release-iphoneos/CADCraft.app" "$OUT/ipa/Payload/"
  (cd "$OUT/ipa" && zip -qry "$IPA" Payload)
  rm -rf "$OUT/ipa"
  echo "Unsigned IPA: $IPA ($(du -h "$IPA" | cut -f1))"
fi

# 4. TestFlight: archive signed for the team (Xcode makes the certificate and profile through the
# API key), then export with destination "upload", which sends the build to App Store Connect.
if [ "$TESTFLIGHT" = 1 ]; then
  AUTH=(-allowProvisioningUpdates -authenticationKeyPath "$ASC_KEY_PATH" -authenticationKeyID "$ASC_KEY_ID"
    -authenticationKeyIssuerID "$ASC_ISSUER_ID")
  ARCHIVE="$OUT/CADCraft.xcarchive"
  rm -rf "$ARCHIVE" "$OUT/export"
  xcodebuild -project "$XC/CADCraft.xcodeproj" -scheme CADCraft -configuration Release \
    -destination 'generic/platform=iOS' -archivePath "$ARCHIVE" "${AUTH[@]}" \
    CODE_SIGN_STYLE=Automatic CODE_SIGN_IDENTITY="Apple Development" DEVELOPMENT_TEAM="$APPLE_TEAM_ID" archive
  cat >"$OUT/ExportOptions.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>method</key><string>app-store-connect</string>
  <key>destination</key><string>upload</string>
  <key>teamID</key><string>$APPLE_TEAM_ID</string>
  <key>signingStyle</key><string>automatic</string>
  <key>manageAppVersionAndBuildNumber</key><false/>
  <key>uploadSymbols</key><false/>
</dict>
</plist>
EOF
  xcodebuild -exportArchive -archivePath "$ARCHIVE" -exportOptionsPlist "$OUT/ExportOptions.plist" \
    -exportPath "$OUT/export" "${AUTH[@]}"
  echo "Uploaded build $CADCRAFT_SHORT_VERSION ($CADCRAFT_BUILD_NUMBER) of $CADCRAFT_BUNDLE_ID to App Store Connect;"
  echo "it shows in TestFlight once Apple has processed it (usually 5-30 minutes)."
fi
