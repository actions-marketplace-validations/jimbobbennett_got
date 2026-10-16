import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[1]
GENERATOR = ROOT / "scripts" / "prepare-release.py"
ASSETS = (
    "got-darwin-aarch64.tar.gz",
    "got-linux-x86_64.tar.gz",
    "got-linux-aarch64.tar.gz",
    "got-windows-x86_64.zip",
    "got-windows-aarch64.zip",
)


class ReleasePackagingTests(unittest.TestCase):
    def test_project_declares_mit_license_and_includes_license_file(self):
        with (ROOT / "Cargo.toml").open("rb") as manifest:
            package = tomllib.load(manifest)["package"]

        self.assertEqual(package["license"], "MIT")
        self.assertIn("MIT License", (ROOT / "LICENSE").read_text(encoding="utf-8"))

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.dist = Path(self.temp_dir.name) / "dist"
        self.dist.mkdir()
        self.contents = {}
        for name in ASSETS:
            content = f"fixture:{name}".encode()
            (self.dist / name).write_bytes(content)
            self.contents[name] = hashlib.sha256(content).hexdigest()

    def tearDown(self):
        self.temp_dir.cleanup()

    def run_generator(self, license_name=None):
        manifest = self.dist.parent / "Cargo.toml"
        license_entry = f'license = "{license_name}"\n' if license_name else ""
        manifest.write_text(
            f'[package]\nname = "got-cli"\nversion = "0.1.0"\n{license_entry}',
            encoding="utf-8",
        )
        if license_name:
            (self.dist.parent / "LICENSE").write_text("fixture license\n", encoding="utf-8")
        return subprocess.run(
            [
                sys.executable,
                str(GENERATOR),
                "--version",
                "0.1.0",
                "--dist",
                str(self.dist),
                "--manifest",
                str(manifest),
            ],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )

    def test_generates_checksums_and_arm64_homebrew_formula(self):
        result = self.run_generator()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(
            self.contents["got-darwin-aarch64.tar.gz"],
            (self.dist / "SHA256SUMS").read_text(),
        )
        formula = (self.dist / "got.rb").read_text()
        self.assertIn("got-darwin-aarch64.tar.gz", formula)
        self.assertIn("https://github.com/cloudless-ai/got/releases/download/v0.1.0/", formula)
        self.assertIn(self.contents["got-darwin-aarch64.tar.gz"], formula)
        self.assertIn("Hardware::CPU.arm?", formula)
        self.assertIn('depends_on "git"', formula)
        self.assertIn('license "MIT"', formula)
        self.assertFalse((self.dist / "CloudlessAI.Got.installer.yaml").exists())
        self.assertIn("WinGet manifests were not generated", result.stdout)

    def test_generates_winget_manifests_for_both_windows_architectures(self):
        result = self.run_generator("MIT")
        self.assertEqual(result.returncode, 0, result.stderr)
        installer_manifest = (self.dist / "CloudlessAI.Got.installer.yaml").read_text()
        self.assertIn("Architecture: x64", installer_manifest)
        self.assertIn("Architecture: arm64", installer_manifest)
        self.assertIn(self.contents["got-windows-x86_64.zip"], installer_manifest)
        self.assertIn(self.contents["got-windows-aarch64.zip"], installer_manifest)
        self.assertEqual(installer_manifest.count("      - PackageIdentifier: Git.Git"), 2)
        self.assertEqual(installer_manifest.count("        MinimumVersion: 2.25.0"), 2)
        locale_manifest = (self.dist / "CloudlessAI.Got.locale.en-US.yaml").read_text()
        self.assertIn('License: "MIT"', locale_manifest)
        self.assertIn("Publisher: Cloudless AI", locale_manifest)
        self.assertIn("PackageIdentifier: CloudlessAI.Got", locale_manifest)
        self.assertIn('PackageUrl: "https://github.com/cloudless-ai/got"', locale_manifest)
        self.assertIn("https://github.com/cloudless-ai/got/releases/download/v0.1.0/", installer_manifest)
        version_manifest = (self.dist / "CloudlessAI.Got.yaml").read_text()
        self.assertIn("ManifestVersion: 1.12.0", version_manifest)
        self.assertIn("ManifestVersion: 1.12.0", locale_manifest)
        self.assertIn("ManifestVersion: 1.12.0", installer_manifest)
        self.assertIn(
            "$schema=https://aka.ms/winget-manifest.version.1.12.0.schema.json",
            version_manifest,
        )
        self.assertIn(
            "$schema=https://aka.ms/winget-manifest.defaultLocale.1.12.0.schema.json",
            locale_manifest,
        )
        self.assertIn(
            "$schema=https://aka.ms/winget-manifest.installer.1.12.0.schema.json",
            installer_manifest,
        )

    def test_updates_action_version_in_readme_and_docs(self):
        root = Path(self.temp_dir.name) / "repo"
        (root / "docs").mkdir(parents=True)
        (root / "README.md").write_text(
            "  - uses: cloudless-ai/got@v1\n",
            encoding="utf-8",
        )
        (root / "docs" / "index.html").write_text(
            "  - uses: cloudless-ai/got@v0.1.0\n",
            encoding="utf-8",
        )
        result = subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts" / "update_action_version.py"),
                "--version",
                "v0.2.0",
                "--root",
                str(root),
            ],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(
            "cloudless-ai/got@v0.2.0",
            (root / "README.md").read_text(encoding="utf-8"),
        )
        self.assertIn(
            "cloudless-ai/got@v0.2.0",
            (root / "docs" / "index.html").read_text(encoding="utf-8"),
        )


if __name__ == "__main__":
    unittest.main()
