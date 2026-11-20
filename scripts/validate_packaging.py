#!/usr/bin/env python3
"""Validate source-controlled package metadata before release packaging."""
from __future__ import annotations

import configparser
import plistlib
import re
import sys
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def main() -> int:
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    version = cargo["package"]["version"]
    require(bool(version), "Cargo.toml package version is empty")
    require(cargo["package"].get("license") == "GPL-3.0-only", "Unexpected package SPDX license")

    lock = tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))
    app_packages = [p for p in lock.get("package", []) if p.get("name") == "jpeg_viewer"]
    require(len(app_packages) == 1, "Cargo.lock must contain exactly one jpeg_viewer package")
    require(app_packages[0]["version"] == version, "Cargo.toml and Cargo.lock application versions differ")

    plist_path = ROOT / "packaging/macos/Info.plist"
    with plist_path.open("rb") as stream:
        plist = plistlib.load(stream)
    require(plist.get("CFBundleIdentifier") == "com.jpegviewer.JpegViewer", "Incorrect macOS bundle identifier")
    require(plist.get("CFBundleExecutable") == "jpeg_viewer", "Incorrect macOS bundle executable")
    require("CFBundleDocumentTypes" in plist, "macOS document types are missing")
    document_types = {
        content_type
        for document in plist["CFBundleDocumentTypes"]
        for content_type in document.get("LSItemContentTypes", [])
    }
    require("public.image" in document_types, "macOS image document type is missing")
    require("public.archive" in document_types, "macOS archive document type is missing")
    require("com.jpegviewer.queue" in document_types, "macOS queue document type is missing")

    xml_files = [
        ROOT / "packaging/linux/com.jpegviewer.JpegViewer.metainfo.xml",
        ROOT / "packaging/flatpak/com.jpegviewer.JpegViewer.metainfo.xml",
        ROOT / "packaging/linux/jpeg-viewer-queue.xml",
    ]
    parsed_xml = {}
    for path in xml_files:
        parsed_xml[path] = ET.parse(path)

    mime_root = parsed_xml[ROOT / "packaging/linux/jpeg-viewer-queue.xml"].getroot()
    mime_ns = {"mime": "http://www.freedesktop.org/standards/shared-mime-info"}
    declared_mime_types = {
        node.attrib.get("type")
        for node in mime_root.findall("mime:mime-type", mime_ns)
    }
    required_custom_mime_types = {
        "application/x-jpeg-viewer-queue",
        "image/qoi",
        "image/x-farbfeld",
        "image/x-dds",
        "image/x-exr",
        "image/x-tga",
        "image/x-camera-raw",
    }
    require(
        required_custom_mime_types <= declared_mime_types,
        "Custom MIME types are missing from jpeg-viewer-queue.xml: "
        + ", ".join(sorted(required_custom_mime_types - declared_mime_types)),
    )

    desktop_files = [
        ROOT / "packaging/linux/com.jpegviewer.JpegViewer.desktop",
        ROOT / "packaging/linux/com.jpegviewer.JpegViewer.deb.desktop",
        ROOT / "packaging/flatpak/com.jpegviewer.JpegViewer.desktop",
    ]
    for path in desktop_files:
        parser = configparser.ConfigParser(interpolation=None, strict=True)
        parser.optionxform = str
        with path.open(encoding="utf-8") as stream:
            parser.read_file(stream)
        require(parser.has_section("Desktop Entry"), f"Missing Desktop Entry section: {path}")
        entry = parser["Desktop Entry"]
        for key in ("Name", "Exec", "Type", "Categories"):
            require(entry.get(key), f"Missing {key} in {path}")
        require(entry.get("Type") == "Application", f"Invalid desktop Type in {path}")

    required_assets = [
        ROOT / "assets/icons/icon.ico",
        ROOT / "assets/icons/icon.png",
        ROOT / "packaging/icons/512x512/com.jpegviewer.JpegViewer.png",
        ROOT / "packaging/windows/installer.iss",
        ROOT / "packaging/linux/install-portable.sh",
        ROOT / "packaging/linux/uninstall-portable.sh",
        ROOT / "LICENSE",
        ROOT / "packaging/linux/debian/copyright",
        ROOT / "QUICK_START.md",
        ROOT / "README.md",
    ]
    for path in required_assets:
        require(path.is_file() and path.stat().st_size > 0, f"Required package asset missing: {path}")

    for script in (
        ROOT / "packaging/linux/debian/postinst",
        ROOT / "packaging/linux/debian/postrm",
        ROOT / "packaging/linux/install-portable.sh",
        ROOT / "packaging/linux/uninstall-portable.sh",
    ):
        require(script.stat().st_mode & 0o111, f"Packaging script is not executable: {script}")

    installer = (ROOT / "packaging/windows/installer.iss").read_text(encoding="utf-8")
    require("PrivilegesRequired=lowest" in installer, "Windows installer must be per-user / non-elevated")
    require("{localappdata}\\Programs\\JPEG Viewer" in installer, "Windows installer must default to a per-user install directory")
    require("VersionInfoVersion={#GetEnv(\"JPEG_VIEWER_FILE_VERSION\")}" in installer,
            "Windows installer must include a numeric setup file version")
    require("VersionInfoProductVersion={#GetEnv(\"JPEG_VIEWER_FILE_VERSION\")}" in installer,
            "Windows installer must include a numeric product file version")
    require('GetEnv("JPEG_VIEWER_FILE_VERSION") == ""' in installer,
            "Windows installer must reject missing file-version metadata")
    assoc_extensions = re.findall(
        r'Subkey: "Software\\Classes\\(\.[^\\"]+)\\OpenWithProgids"',
        installer,
    )
    require(len(assoc_extensions) == len(set(assoc_extensions)),
            "Windows installer contains duplicate Open With extension registrations")
    require('Subkey: "Software\\Classes\\.jvq\\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Queue"' in installer,
            "JPEG Viewer queue files must use a dedicated queue ProgID")
    image_core = (ROOT / "src/imaging/core.rs").read_text(encoding="utf-8")
    extension_block = image_core.split("pub const fn extensions(self)", 1)[1].split("pub const fn preferred_extension", 1)[0]
    supported_extensions = set(re.findall(r'"([a-z0-9]+)"', extension_block))
    registered_extensions = {extension.removeprefix(".") for extension in assoc_extensions}
    require(supported_extensions <= registered_extensions,
            "Windows Open With registrations are missing supported image extensions: "
            + ", ".join(sorted(supported_extensions - registered_extensions)))

    toolchain = (ROOT / "rust-toolchain.toml").read_text(encoding="utf-8")
    require('channel = "1.99.1"' in toolchain, "Rust release toolchain is not pinned")
    flatpak_manifest = (ROOT / "packaging/flatpak/com.jpegviewer.JpegViewer.yml").read_text(encoding="utf-8")
    require("RUSTUP_TOOLCHAIN: stable" in flatpak_manifest,
            "Flatpak must select the Rust toolchain supplied by its SDK extension")

    print(f"Package metadata validated for JPEG Viewer {version}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"Packaging validation failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
