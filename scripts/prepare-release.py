#!/usr/bin/env python3
"""Generate checksums and package-manager metadata for a release bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import tomllib

REPOSITORY = "cloudless-ai/got"
WINDOWS_ASSETS = (
    ("x64", "got-windows-x86_64.zip"),
    ("arm64", "got-windows-aarch64.zip"),
)
RELEASE_ASSETS = (
    "got-darwin-aarch64.tar.gz",
    "got-linux-x86_64.tar.gz",
    "got-linux-aarch64.tar.gz",
    *(asset for _, asset in WINDOWS_ASSETS),
)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def yaml_string(value: str) -> str:
    return json.dumps(value, ensure_ascii=True)


def homebrew_formula(version: str, checksum: str) -> str:
    url = (
        f"https://github.com/{REPOSITORY}/releases/download/v{version}/"
        "got-darwin-aarch64.tar.gz"
    )
    return f'''class Got < Formula
  desc "Fast, folder-scoped Git checkouts"
  homepage "https://github.com/{REPOSITORY}"
  license "MIT"
  version "{version}"
  url "{url}"
  sha256 "{checksum}"

  depends_on "git"

  def install
    odie "got is currently packaged for Apple Silicon only" unless Hardware::CPU.arm?
    bin.install "got"
  end

  test do
    assert_match "got #{{version}}", shell_output("#{{bin}}/got --version")
  end
end
'''


def winget_manifests(version: str, license_name: str, checksums: dict[str, str]) -> dict[str, str]:
    identifier = "CloudlessAI.Got"
    schema_version = "1.12.0"
    common = [
        f"PackageIdentifier: {identifier}",
        f"PackageVersion: {version}",
    ]
    locale_fields = [
        f"# yaml-language-server: $schema=https://aka.ms/winget-manifest.defaultLocale.{schema_version}.schema.json",
        *common,
        "PackageLocale: en-US",
        "Publisher: Cloudless AI",
        "PackageName: got",
        "ShortDescription: Fast, folder-scoped Git checkouts",
        f"License: {yaml_string(license_name)}",
        f"PackageUrl: {yaml_string(f'https://github.com/{REPOSITORY}')}",
        "ManifestType: defaultLocale",
        f"ManifestVersion: {schema_version}",
    ]
    installers = [
        f"# yaml-language-server: $schema=https://aka.ms/winget-manifest.installer.{schema_version}.schema.json",
        *common,
        "Installers:",
    ]
    for architecture, asset in WINDOWS_ASSETS:
        url = f"https://github.com/{REPOSITORY}/releases/download/v{version}/{asset}"
        installers.extend(
            [
                f"- Architecture: {architecture}",
                "  InstallerType: zip",
                f"  InstallerUrl: {yaml_string(url)}",
                f"  InstallerSha256: {checksums[asset]}",
                "  NestedInstallerType: portable",
                "  NestedInstallerFiles:",
                "  - RelativeFilePath: got.exe",
                "    PortableCommandAlias: got",
                "  Dependencies:",
                "    PackageDependencies:",
                "      - PackageIdentifier: Git.Git",
                "        MinimumVersion: 2.25.0",
            ]
        )
    version_manifest = [
        f"# yaml-language-server: $schema=https://aka.ms/winget-manifest.version.{schema_version}.schema.json",
        *common,
        "DefaultLocale: en-US",
        "ManifestType: version",
        f"ManifestVersion: {schema_version}",
    ]
    installers.extend(["ManifestType: installer", f"ManifestVersion: {schema_version}"])
    return {
        f"{identifier}.yaml": "\n".join(version_manifest) + "\n",
        f"{identifier}.locale.en-US.yaml": "\n".join(locale_fields) + "\n",
        f"{identifier}.installer.yaml": "\n".join(installers) + "\n",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True, help="release version without the leading v")
    parser.add_argument("--dist", type=Path, required=True, help="directory containing release archives")
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path("Cargo.toml"),
        help="Cargo manifest to read package version and license from",
    )
    args = parser.parse_args()

    if not args.version or any(char not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-" for char in args.version):
        parser.error("version must contain only letters, numbers, '.', '_' or '-'")
    args.dist.mkdir(parents=True, exist_ok=True)

    with args.manifest.open("rb") as manifest:
        package = tomllib.load(manifest)["package"]
    cargo_version = package["version"]
    if args.version != cargo_version:
        parser.error(
            f"release tag version {args.version!r} does not match Cargo.toml version {cargo_version!r}"
        )

    missing = [name for name in RELEASE_ASSETS if not (args.dist / name).is_file()]
    if missing:
        parser.error(f"missing release archives: {', '.join(missing)}")

    checksums = {name: sha256(args.dist / name) for name in RELEASE_ASSETS}
    (args.dist / "SHA256SUMS").write_text(
        "".join(f"{checksums[name]}  {name}\n" for name in sorted(checksums)),
        encoding="utf-8",
    )
    (args.dist / "got.rb").write_text(
        homebrew_formula(args.version, checksums["got-darwin-aarch64.tar.gz"]),
        encoding="utf-8",
    )

    license_name = package.get("license", "").strip()
    if license_name:
        license_file = package.get("license-file")
        license_candidates = (
            [args.manifest.parent / license_file]
            if license_file
            else [
                args.manifest.parent / name
                for name in ("LICENSE", "LICENSE.md", "LICENSE.txt")
            ]
        )
        if not any(path.is_file() for path in license_candidates):
            parser.error(
                "package license is declared but no license file exists; add one "
                "before generating WinGet manifests"
            )
        for filename, content in winget_manifests(args.version, license_name, checksums).items():
            (args.dist / filename).write_text(content, encoding="utf-8")
        print("Generated WinGet manifests; submit them to microsoft/winget-pkgs to publish.")
    else:
        print(
            "::warning::WinGet manifests were not generated. Declare the package "
            "license in Cargo.toml and add its corresponding license file first."
        )
    print(f"Prepared release metadata for v{args.version}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
