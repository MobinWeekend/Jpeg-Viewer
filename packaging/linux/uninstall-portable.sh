#!/bin/sh
set -eu

DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
APP_DIR="$HOME/.local/opt/jpeg-viewer"
APPLICATIONS_DIR="$DATA_HOME/applications"
ICON_DIR="$DATA_HOME/icons/hicolor/512x512/apps"
MIME_PACKAGES_DIR="$DATA_HOME/mime/packages"
DESKTOP_ID="com.jpegviewer.JpegViewer"

rm -rf -- "$APP_DIR"
rm -f -- \
    "$APPLICATIONS_DIR/$DESKTOP_ID.desktop" \
    "$ICON_DIR/$DESKTOP_ID.png" \
    "$MIME_PACKAGES_DIR/jpeg-viewer-queue.xml"

if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database "$DATA_HOME/mime" >/dev/null 2>&1 || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APPLICATIONS_DIR" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f "$DATA_HOME/icons/hicolor" >/dev/null 2>&1 || true
fi

printf 'JPEG Viewer portable installation removed. User settings were preserved.\n'
