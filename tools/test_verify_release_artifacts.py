#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import io
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
import zipfile


MODULE_PATH = Path(__file__).with_name("verify_release_artifacts.py")
SPEC = importlib.util.spec_from_file_location("verify_release_artifacts", MODULE_PATH)
assert SPEC and SPEC.loader
verify_release_artifacts = importlib.util.module_from_spec(SPEC)
sys.modules["verify_release_artifacts"] = verify_release_artifacts
SPEC.loader.exec_module(verify_release_artifacts)


class VerifyReleaseArtifactsTests(unittest.TestCase):
    version = "0.1.11"
    readme = """# turbo-picard

The current source release is `0.1.11`.

```bash
docker run --rm ghcr.io/dnncha/turbo-picard:0.1.11 --version
```
"""

    def make_root(self, root: Path) -> None:
        (root / "Cargo.toml").write_text(
            '[workspace.package]\nversion = "0.1.11"\n', encoding="utf-8"
        )
        (root / "README.md").write_text(self.readme, encoding="utf-8")

    def metadata(self) -> bytes:
        return (
            "Metadata-Version: 2.4\n"
            "Name: turbo-picard\n"
            "Version: 0.1.11\n"
            "Description-Content-Type: text/markdown; charset=UTF-8\n"
            "\n"
            + self.readme
        ).encode()

    def write_wheel(
        self,
        path: Path,
        description: str | None = None,
        binary_architecture: str = "x86_64",
    ) -> None:
        metadata = self.metadata() if description is None else self.metadata().replace(
            self.readme.encode(), description.encode()
        )
        binary = bytearray(64)
        if binary_architecture == "x86_64":
            binary[:4] = b"\x7fELF"
            binary[18:20] = (62).to_bytes(2, "little")
        elif binary_architecture == "arm64":
            binary[:4] = b"\xcf\xfa\xed\xfe"
            binary[4:8] = (0x0100000C).to_bytes(4, "little")
        else:
            raise ValueError(f"unsupported test architecture: {binary_architecture}")
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr("turbo_picard-0.1.11.data/scripts/turbo-picard", binary)
            archive.writestr("turbo_picard-0.1.11.data/scripts/picard", binary)
            archive.writestr("turbo_picard-0.1.11.dist-info/METADATA", metadata)
            archive.writestr("turbo_picard-0.1.11.dist-info/WHEEL", "Wheel-Version: 1.0\n")
            archive.writestr("turbo_picard-0.1.11.dist-info/RECORD", "")

    def write_sdist(self, path: Path) -> None:
        root = "turbo_picard-0.1.11"
        with tarfile.open(path, "w:gz") as archive:
            for name, content in {
                f"{root}/README.md": self.readme,
                f"{root}/PKG-INFO": self.metadata().decode(),
                f"{root}/Cargo.toml": '[workspace.package]\nversion = "0.1.11"\n',
                f"{root}/pyproject.toml": "[project]\nname = 'turbo-picard'\n",
            }.items():
                data = content.encode()
                info = tarfile.TarInfo(name)
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))

    def test_accepts_matching_wheel_and_sdist(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "repo"
            dist = root / "dist"
            dist.mkdir(parents=True)
            self.make_root(root)
            self.write_wheel(
                dist
                / "turbo_picard-0.1.11-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl"
            )
            self.write_sdist(dist / "turbo_picard-0.1.11.tar.gz")
            self.assertEqual([], verify_release_artifacts.collect_errors(dist, root))

    def test_rejects_stale_container_marker(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "repo"
            dist = root / "dist"
            dist.mkdir(parents=True)
            self.make_root(root)
            (root / "README.md").write_text(
                self.readme.replace("0.1.11", "0.1.10"), encoding="utf-8"
            )
            wheel = (
                dist
                / "turbo_picard-0.1.11-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl"
            )
            self.write_wheel(wheel)
            errors = verify_release_artifacts.collect_errors(dist, root)
            self.assertIn(
                "README.md must contain current release marker: The current source release is `0.1.11`",
                errors,
            )

    def test_rejects_long_description_drift(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "repo"
            dist = root / "dist"
            dist.mkdir(parents=True)
            self.make_root(root)
            self.write_wheel(
                dist
                / "turbo_picard-0.1.11-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
                description=self.readme + "\nUnexpected drift.\n",
            )
            errors = verify_release_artifacts.collect_errors(dist, root)
            self.assertIn(
                "turbo_picard-0.1.11-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl "
                "long description must match "
                "the checked-out README.md",
                errors,
            )

    def test_rejects_binary_platform_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "repo"
            dist = root / "dist"
            dist.mkdir(parents=True)
            self.make_root(root)
            wheel = (
                dist
                / "turbo_picard-0.1.11-py3-none-manylinux_2_17_aarch64.manylinux2014_aarch64.whl"
            )
            self.write_wheel(wheel)
            errors = verify_release_artifacts.collect_errors(dist, root)
            self.assertIn(
                "turbo_picard-0.1.11-py3-none-manylinux_2_17_aarch64.manylinux2014_aarch64.whl "
                "entrypoint architecture x86_64 must match wheel platform aarch64",
                errors,
            )

    def test_accepts_macos_arm64_binary(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "repo"
            dist = root / "dist"
            dist.mkdir(parents=True)
            self.make_root(root)
            wheel = dist / "turbo_picard-0.1.11-py3-none-macosx_11_0_arm64.whl"
            self.write_wheel(wheel, binary_architecture="arm64")
            self.assertEqual([], verify_release_artifacts.collect_errors(dist, root))


if __name__ == "__main__":
    unittest.main()
