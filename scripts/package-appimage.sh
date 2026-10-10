#!/usr/bin/env bash
set -euo pipefail
APPIMAGE_NAME="recode"
APPIMAGE_PATH="target/release/recode"
mkdir -p AppDir/usr/bin
cp "$APPIMAGE_PATH" "AppDir/usr/bin/$APPIMAGE_NAME"
cd AppDir
linuxdeploy-x86_64.AppImage --appdir . --output appimage
mv recode*.AppImage ../"$APPIMAGE_NAME".AppImage
cd -
rm -rf AppDir
