#!/bin/sh
set -eu

SOURCE_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
APP_DIR="${HOME}/.local/opt/jpeg-viewer"
APPlications_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICON_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/512x512/apps"
DESKTOP_ID="com.jpegviewer.JpegViewer"

mkdir -p "$APP_DIR" "$APPlications_DIR" "$ICON_DIR"
install -m 755 "$SOURCE_DIR/jpeg_viewer" "$APP_DIR/jpeg_viewer"
install -m 644 "$SOURCE_DIR/icon.png" "$ICON_DIR/$DESKTOP_ID.png"

# Quote absolute paths for the Desktop Entry Exec field.
printf '[Desktop Entry]\nName=JPEG Viewer\nGenericName=Image Viewer\nComment=Fast and lightweight GPU-accelerated image viewer\nExec="%s" %%F\nIcon=%s\nTerminal=false\nType=Application\nCategories=Graphics;Viewer;Photography;\nStartupNotify=true\nStartupWMClass=%s\n' \
  "$APP_DIR/jpeg_viewer" "$ICON_DIR/$DESKTOP_ID.png" "$DESKTOP_ID" \
  > "$APPlications_DIR/$DESKTOP_ID.desktop"

chmod 644 "$APPlications_DIR/$DESKTOP_ID.desktop"
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$APPlications_DIR" >/dev/null 2>&1 || true
fi
printf 'Installed JPEG Viewer. Launch it from your applications menu.\n'
