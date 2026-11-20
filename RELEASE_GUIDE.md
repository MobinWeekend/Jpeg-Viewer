# Release guide

The workflow supports tag-triggered releases and manual builds. Tag pushes and manual full releases validate the source on Linux and Windows, build every platform on native runners, verify the complete asset set, generate checksums and an SPDX SBOM, create GitHub provenance attestations, and publish the GitHub Release. Manual artifacts-only runs can build just the selected platform/architecture targets.

## Before the first release with this workflow

1. Ensure `Cargo.lock` is committed. It is required for reproducible `--locked` builds and for the offline Flatpak dependency source bundle. The workflow deliberately fails early if Git does not track it.
2. Commit the source, packaging metadata, and workflow changes before creating the tag.
3. Confirm the GitHub repository allows Actions to create releases. The workflow uses the repository `GITHUB_TOKEN`; no personal access token is required.
4. Make sure the default branch and release tag contain the same committed `Cargo.lock` and packaging scripts.

If `Cargo.lock` was previously ignored and is still untracked, add it explicitly once:

```powershell
git add -f Cargo.lock
git add .gitignore
git status
```

Commit the lockfile and `.gitignore` before tagging.

## Build artifacts manually or publish a release

The same workflow can be started from **Actions → Build and Release → Run workflow**. Select the branch or commit to build, then choose a mode:

- **Artifacts only** (default): builds the requested platform/architecture targets and uploads them as downloadable GitHub Actions artifacts. It does not create a Git tag or publish a GitHub Release. Artifacts currently remain available for 7 days.
- **Full release**: ignores platform checkboxes, builds every supported platform and architecture, verifies the complete asset set, generates checksums and an SBOM, and publishes a GitHub Release. Choose a new, unused version for this mode; the workflow checks that the tag does not already exist.

For fast artifact-only tests, set **Platform selection** to `selected` and check only the exact targets you want. Targets can be mixed freely in one run, for example **Arch x64** plus **Debian ARM64**, or just **Windows x64**. The Windows installer is generated only for a selected Windows architecture. macOS targets are listed separately as Intel x64 and Apple Silicon ARM64.

Leave **Run validation** unchecked for the fastest build-only test. Enable it to run the Linux and Windows validation/test jobs before the selected builds. The default **Platform selection = all** preserves the previous behavior. Tag-triggered releases and manual **Full release** runs always build the complete platform set regardless of the checkboxes.

The manual workflow accepts a version and separate Docker image references for x64 and ARM64 Arch builds. It builds the selected commit from the branch/ref selector, so artifact tests do not require a pre-existing version tag. The image fields accept a Docker tag or an immutable `@sha256:` digest.

## Create a release

A normal release can still be triggered by pushing a SemVer tag beginning with `v`:

```powershell
git tag -a v0.2.0 -m "JPEG Viewer 0.2.0"
git push origin v0.2.0
```

A tag containing a SemVer prerelease suffix, such as `v0.2.0-rc.1`, is published as a prerelease. Build metadata such as `+build.5` is accepted and synchronized across package metadata.

The workflow updates the package version in its temporary CI checkout from the tag. It does not commit a version change back to the repository. The dependency graph in `Cargo.lock` remains unchanged; only the root `jpeg_viewer` package version is synchronized. Debian, RPM, and Arch package versions are separately normalized so prereleases sort before the corresponding stable release; the source/application version remains the exact SemVer tag. Windows/macOS numeric file/bundle version fields use only the `MAJOR.MINOR.PATCH` components. The workflow rejects numeric components larger than 65535 because Windows version-resource components are 16-bit.

## Windows installation

The release includes x64 and ARM64 setup installers and portable ZIPs. The Inno Setup 6.7.4 installer uses `PrivilegesRequired=lowest` and installs under the current user's `%LOCALAPPDATA%\Programs\JPEG Viewer` directory. If you use Inno Setup commercially, review its current licensing terms; the upstream project requests a paid license for commercial use. It does not request administrator privileges, writes optional file-open registrations under `HKCU`, and does not change default file associations.

The installer preserves user settings when the application is uninstalled. Windows executables and installers are currently unsigned; add Authenticode signing credentials to CI before claiming verified publisher identity or expecting SmartScreen reputation.

## macOS signing

The workflow currently creates `.app`, ZIP, and drag-to-Applications DMG packages. They are unsigned and not notarized. To remove Gatekeeper friction, configure a Developer ID Application certificate, Apple notarization credentials, and the associated CI secrets, then add signing, notarization, and ticket stapling before publishing.

## Verify downloaded assets

On Linux/macOS:

```bash
sha256sum -c SHA256SUMS
```

On Windows PowerShell, calculate a file hash and compare it with the matching entry in `SHA256SUMS`:

```powershell
Get-FileHash .\JPEG-Viewer-Setup-x64.exe -Algorithm SHA256
```

GitHub build provenance is attached to the release artifacts. With GitHub CLI installed, verify an asset using:

```bash
gh attestation verify JPEG-Viewer-Portable-x64.zip -R MobinWeekend/Jpeg-Viewer
```

The release also includes `JPEG-Viewer-SBOM.spdx.json`, an SPDX software bill of materials generated from the tagged source and its dependency metadata.

## Local Windows installer build

1. Build the release executable for the architecture you are targeting.
2. Copy it to a directory and name it exactly `JPEG-Viewer-Windows-x64.exe` or `JPEG-Viewer-Windows-ARM64.exe`.
3. Set the three environment variables expected by `packaging/windows/installer.iss` and compile it with Inno Setup Compiler (`ISCC.exe`):

```powershell
$env:JPEG_VIEWER_VERSION = "0.2.0"
$env:JPEG_VIEWER_ARCH = "x64"
$env:JPEG_VIEWER_FILE_VERSION = "0.2.0.0"
$env:JPEG_VIEWER_BINARY_DIR = (Resolve-Path .\installer-input\x64).Path
& "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" .\packaging\windows\installer.iss
```

The compiled installer is written to `dist/windows-installers`. The workflow pins the Rust toolchain to 1.99.1 to match the verified project build environment; update `rust-toolchain.toml` and all CI toolchain references together when deliberately upgrading Rust.
