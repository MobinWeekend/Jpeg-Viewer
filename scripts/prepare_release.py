#!/usr/bin/env python3
"""Synchronize package metadata with the Git release tag.

This script intentionally edits only the root application package version in
Cargo.toml/Cargo.lock. Dependency versions and checksums remain untouched.
"""
from __future__ import annotations

import datetime as _datetime
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
_NUM = r"(?:0|[1-9]\d*)"
_PRE_ID = r"(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)"
SEMVER = re.compile(
    rf"^v?({_NUM})\.({_NUM})\.({_NUM})"
    rf"(?:-({_PRE_ID}(?:\.{_PRE_ID})*))?"
    rf"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
)


def replace_one(pattern: str, replacement: str, text: str, label: str) -> str:
    updated, count = re.subn(pattern, replacement, text, count=1, flags=re.MULTILINE)
    if count != 1:
        raise RuntimeError(f"Expected exactly one {label} entry; found {count}")
    return updated


def release_date() -> str:
    try:
        value = subprocess.check_output(
            ["git", "show", "-s", "--format=%cs", "HEAD"],
            cwd=ROOT,
            text=True,
            stderr=subprocess.DEVNULL,
        ).strip()
        _datetime.date.fromisoformat(value)
        return value
    except Exception:
        return _datetime.datetime.now(_datetime.timezone.utc).date().isoformat()


def update_appstream(path: Path, version: str, date: str) -> None:
    text = path.read_text(encoding="utf-8")
    release_tag = f'<release version="{version}" date="{date}"/>'

    # Make re-runs idempotent: update the current release entry if it exists.
    # Replace either a self-closing entry or a block entry with nested notes.
    existing = re.compile(
        rf'<release\b(?=[^>]*\bversion="{re.escape(version)}")[^>]*>.*?</release>|'
        rf'<release\b(?=[^>]*\bversion="{re.escape(version)}")[^>]*/>',
        re.DOTALL,
    )
    if existing.search(text):
        text = existing.sub(release_tag, text, count=1)
    else:
        text, count = re.subn(r"(<releases>\s*)", r"\1        " + release_tag + "\n", text, count=1)
        if count != 1:
            raise RuntimeError(f"Missing <releases> section in {path}")

    path.write_text(text, encoding="utf-8", newline="\n")


def main() -> int:
    if len(sys.argv) != 2:
        print("Usage: prepare_release.py vMAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]", file=sys.stderr)
        return 2

    tag = sys.argv[1]
    match = SEMVER.fullmatch(tag)
    if not match:
        raise SystemExit(f"Invalid release tag: {tag!r}; expected a SemVer vMAJOR.MINOR.PATCH tag")

    version = tag.removeprefix("v")
    major, minor, patch, prerelease, build_metadata = match.groups()
    numeric_parts = tuple(int(value) for value in (major, minor, patch))
    if any(value > 65535 for value in numeric_parts):
        raise SystemExit("Major, minor, and patch version components must fit Windows' 16-bit file-version fields")

    numeric_bundle_version = f"{major}.{minor}.{patch}"

    # Package managers have different version-comparison rules. Keep the exact
    # SemVer version in Cargo/AppStream, but derive package-native forms so a
    # prerelease sorts before its final stable release.
    arch_version = numeric_bundle_version
    rpm_version = numeric_bundle_version
    deb_upstream_version = numeric_bundle_version
    if prerelease:
        # ALPM treats an alphabetic suffix (e.g. 1.2.3rc.1) as older than
        # the bare final release; an underscore suffix sorts after stable.
        arch_version += f"pre{prerelease.replace('-', '_')}"
        rpm_version += f"~{prerelease.replace('-', '.')}"
        deb_upstream_version += f"~{prerelease}"
    if build_metadata:
        arch_version += f"+{build_metadata.replace('-', '_')}"
        rpm_version += f".{build_metadata.replace('-', '.')}"
        deb_upstream_version += f"+{build_metadata.replace('-', '.')}"
    deb_version = f"{deb_upstream_version}-1"
    windows_file_version = f"{numeric_bundle_version}.0"
    date = release_date()

    # Update only the [package] version in Cargo.toml.
    cargo_toml = ROOT / "Cargo.toml"
    manifest = cargo_toml.read_text(encoding="utf-8")
    package_block = re.search(r"(?ms)^\[package\]\s*$.*?(?=^\[|\Z)", manifest)
    if not package_block:
        raise RuntimeError("Cargo.toml has no [package] section")
    block = package_block.group(0)
    block = replace_one(r'^version\s*=\s*"[^"]+"\s*$', f'version = "{version}"', block, "Cargo.toml [package].version")
    manifest = manifest[: package_block.start()] + block + manifest[package_block.end() :]
    cargo_toml.write_text(manifest, encoding="utf-8", newline="\n")

    # Update only the root application's lockfile entry; dependency locks stay fixed.
    cargo_lock = ROOT / "Cargo.lock"
    lock = cargo_lock.read_text(encoding="utf-8")
    lock_block = re.search(
        r'(?ms)^\[\[package\]\]\n(?:(?!^\[\[package\]\]).)*?^name = "jpeg_viewer"\n(?:(?!^\[\[package\]\]).)*?(?=^\[\[package\]\]|\Z)',
        lock,
    )
    if not lock_block:
        raise RuntimeError("Cargo.lock has no jpeg_viewer package entry; commit Cargo.lock before releasing")
    block = lock_block.group(0)
    block = replace_one(r'^version\s*=\s*"[^"]+"\s*$', f'version = "{version}"', block, "Cargo.lock jpeg_viewer version")
    lock = lock[: lock_block.start()] + block + lock[lock_block.end() :]
    cargo_lock.write_text(lock, encoding="utf-8", newline="\n")

    # Arch package versions cannot use SemVer's hyphen separator. Keep the full
    # Git tag separately so the exact tagged source is still built.
    pkgbuild = ROOT / "packaging/arch/PKGBUILD"
    if pkgbuild.exists():
        text = pkgbuild.read_text(encoding="utf-8")
        text = replace_one(r"^pkgver=.*$", f"pkgver={arch_version}", text, "PKGBUILD pkgver")
        text = replace_one(r"^_release_tag=.*$", f'_release_tag="{tag}"', text, "PKGBUILD _release_tag")
        pkgbuild.write_text(text, encoding="utf-8", newline="\n")

    # Apple's bundle version fields are numeric dotted components. Keep the
    # SemVer suffix in Cargo/app metadata; use MAJOR.MINOR.PATCH in Info.plist.
    info_plist = ROOT / "packaging/macos/Info.plist"
    if info_plist.exists():
        text = info_plist.read_text(encoding="utf-8")
        text = replace_one(
            r'(<key>CFBundleVersion</key>\s*<string>)[^<]*(</string>)',
            rf"\g<1>{numeric_bundle_version}\2",
            text,
            "Info.plist CFBundleVersion",
        )
        text = replace_one(
            r'(<key>CFBundleShortVersionString</key>\s*<string>)[^<]*(</string>)',
            rf"\g<1>{numeric_bundle_version}\2",
            text,
            "Info.plist CFBundleShortVersionString",
        )
        info_plist.write_text(text, encoding="utf-8", newline="\n")

    update_appstream(ROOT / "packaging/linux/com.jpegviewer.JpegViewer.metainfo.xml", version, date)
    update_appstream(ROOT / "packaging/flatpak/com.jpegviewer.JpegViewer.metainfo.xml", version, date)

    env_file = os.environ.get("GITHUB_ENV")
    if env_file:
        with open(env_file, "a", encoding="utf-8", newline="\n") as output:
            output.write(f"JPEG_VIEWER_VERSION={version}\n")
            output.write(f"JPEG_VIEWER_ARCH_VERSION={arch_version}\n")
            output.write(f"JPEG_VIEWER_RPM_VERSION={rpm_version}\n")
            output.write(f"JPEG_VIEWER_DEB_VERSION={deb_version}\n")
            output.write(f"JPEG_VIEWER_BUNDLE_VERSION={numeric_bundle_version}\n")
            output.write(f"JPEG_VIEWER_FILE_VERSION={windows_file_version}\n")
            output.write(f"JPEG_VIEWER_IS_PRERELEASE={'true' if prerelease else 'false'}\n")
            output.write(f"JPEG_VIEWER_RELEASE_DATE={date}\n")

    print(
        f"Prepared release {tag} (Cargo={version}, Arch={arch_version}, "
        f"Debian={deb_version}, RPM={rpm_version}, macOS={numeric_bundle_version}, date={date})"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"Release version preparation failed: {error}", file=sys.stderr)
        raise SystemExit(1)
