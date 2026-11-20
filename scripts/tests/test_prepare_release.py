"""Regression tests for platform-specific release version normalization."""
from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "prepare_release.py"
SOURCE_ROOT = SCRIPT.parents[1]


class PrepareReleaseVersionTests(unittest.TestCase):
    def run_release_helper(self, tag: str) -> tuple[int, str]:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "repo"
            root.mkdir()
            for name in ("Cargo.toml", "Cargo.lock", "packaging", "scripts"):
                source = SOURCE_ROOT / name
                destination = root / name
                if source.is_dir():
                    import shutil
                    shutil.copytree(source, destination)
                else:
                    import shutil
                    shutil.copy2(source, destination)
            env = os.environ.copy()
            env["GITHUB_ENV"] = str(root / "github-env.txt")
            result = subprocess.run(
                [sys.executable, str(root / "scripts/prepare_release.py"), tag],
                cwd=root,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
            return result.returncode, result.stdout + (root / "github-env.txt").read_text() if (root / "github-env.txt").exists() else result.stdout

    def test_stable_release_versions(self) -> None:
        code, output = self.run_release_helper("v1.2.3")
        self.assertEqual(code, 0, output)
        self.assertIn("JPEG_VIEWER_DEB_VERSION=1.2.3-1", output)
        self.assertIn("JPEG_VIEWER_RPM_VERSION=1.2.3", output)
        self.assertIn("JPEG_VIEWER_FILE_VERSION=1.2.3.0", output)
        self.assertIn("JPEG_VIEWER_IS_PRERELEASE=false", output)

    def test_prerelease_sorts_before_stable_for_debian_and_rpm(self) -> None:
        code, output = self.run_release_helper("v1.2.3-rc.1")
        self.assertEqual(code, 0, output)
        self.assertIn("JPEG_VIEWER_DEB_VERSION=1.2.3~rc.1-1", output)
        self.assertIn("JPEG_VIEWER_RPM_VERSION=1.2.3~rc.1", output)
        self.assertIn('JPEG_VIEWER_ARCH_VERSION=1.2.3prerc.1', output)

    def test_build_metadata_is_normalized_for_platform_packages(self) -> None:
        code, output = self.run_release_helper("v1.2.3-rc-1+ci-build.5")
        self.assertEqual(code, 0, output)
        self.assertIn("JPEG_VIEWER_DEB_VERSION=1.2.3~rc-1+ci.build.5-1", output)
        self.assertIn("JPEG_VIEWER_RPM_VERSION=1.2.3~rc.1.ci.build.5", output)
        self.assertIn("JPEG_VIEWER_ARCH_VERSION=1.2.3prerc_1+ci_build.5", output)
        self.assertIn("JPEG_VIEWER_IS_PRERELEASE=true", output)

    def test_build_metadata_hyphen_does_not_make_stable_tag_prerelease(self) -> None:
        code, output = self.run_release_helper("v1.2.3+ci-build.5")
        self.assertEqual(code, 0, output)
        self.assertIn("JPEG_VIEWER_IS_PRERELEASE=false", output)

    def test_rejects_windows_version_components_above_16_bit(self) -> None:
        code, output = self.run_release_helper("v65536.0.0")
        self.assertNotEqual(code, 0)
        self.assertIn("16-bit file-version fields", output)


if __name__ == "__main__":
    unittest.main()
