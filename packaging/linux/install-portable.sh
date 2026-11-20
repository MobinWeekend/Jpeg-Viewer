#!/bin/sh
set -eu

SOURCE_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
APP_DIR="$HOME/.local/opt/jpeg-viewer"
APPLICATIONS_DIR="$DATA_HOME/applications"
ICON_DIR="$DATA_HOME/icons/hicolor/512x512/apps"
MIME_PACKAGES_DIR="$DATA_HOME/mime/packages"
DESKTOP_ID="com.jpegviewer.JpegViewer"

for required in jpeg_viewer icon.png com.jpegviewer.JpegViewer.desktop jpeg-viewer-queue.xml; do
    if [ ! -s "$SOURCE_DIR/$required" ]; then
        printf 'Required portable package file is missing: %s\n' "$required" >&2
        exit 1
    fi
done

mkdir -p "$APP_DIR" "$APPLICATIONS_DIR" "$ICON_DIR" "$MIME_PACKAGES_DIR"
install -m 755 "$SOURCE_DIR/jpeg_viewer" "$APP_DIR/jpeg_viewer"
install -m 644 "$SOURCE_DIR/icon.png" "$ICON_DIR/$DESKTOP_ID.png"
install -m 644 "$SOURCE_DIR/jpeg-viewer-queue.xml" "$MIME_PACKAGES_DIR/jpeg-viewer-queue.xml"

# Preserve the package's MIME declarations while replacing only the executable
# and icon locations with absolute paths for this user's installation.
awk -v exe="$APP_DIR/jpeg_viewer" -v icon="$DESKTOP_ID" '
    /^Exec=/ { printf "Exec=\"%s\" %%F\n", exe; next }
    /^TryExec=/ { next }
    /^Icon=/ { printf "Icon=%s\n", icon; next }
    { print }
' "$SOURCE_DIR/com.jpegviewer.JpegViewer.desktop" > "$APPLICATIONS_DIR/$DESKTOP_ID.desktop"
chmod 644 "$APPLICATIONS_DIR/$DESKTOP_ID.desktop"

if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database "$DATA_HOME/mime" >/dev/null 2>&1 || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APPLICATIONS_DIR" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f "$DATA_HOME/icons/hicolor" >/dev/null 2>&1 || true
fi

printf 'JPEG Viewer installed for the current user.\n'
printf 'Application: %s\n' "$APP_DIR/jpeg_viewer"
printf 'Uninstall:   %s/uninstall.sh\n' "$SOURCE_DIR"
printf 'Your user settings are stored separately and are not changed.\n'
