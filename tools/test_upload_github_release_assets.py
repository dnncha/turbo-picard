from __future__ import annotations

import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from tools import upload_github_release_assets as upload


class FakeResponse:
    def __init__(self, payload: dict):
        self.payload = json.dumps(payload).encode("utf-8")

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        return False

    def read(self) -> bytes:
        return self.payload


class UploadGitHubReleaseAssetsTests(unittest.TestCase):
    def make_release_assets(self, root: Path) -> Path:
        (root / "Cargo.toml").write_text(
            '[workspace.package]\nversion = "0.1.14"\n', encoding="utf-8"
        )
        assets = root / "assets"
        assets.mkdir()
        distribution_names = [
            "turbo_picard-0.1.14-py3-none-manylinux_x86_64.whl",
            "turbo_picard-0.1.14-py3-none-manylinux_aarch64.whl",
            "turbo_picard-0.1.14-py3-none-macosx_arm64.whl",
            "turbo_picard-0.1.14-py3-none-macosx_x86_64.whl",
            "turbo_picard-0.1.14.tar.gz",
        ]
        for name in distribution_names:
            (assets / name).write_bytes(name.encode("utf-8"))
        with (assets / "SHA256SUMS.txt").open("w", encoding="utf-8") as stream:
            for name in distribution_names:
                path = assets / name
                stream.write(f"{upload.asset_digest(path)}  ./{name}\n")
        (assets / "GITHUB_SOURCE_SHA256.txt").write_text(
            f"{'a' * 64}  v0.1.14.tar.gz\n", encoding="utf-8"
        )
        (assets / "turbo-picard-release-manifest.json").write_text(
            json.dumps({"workspace_version": "0.1.14"}), encoding="utf-8"
        )
        return assets

    def release_payload(self, assets: list[dict] | None = None) -> dict:
        return {
            "tag_name": "v0.1.14",
            "draft": False,
            "upload_url": "https://uploads.github.com/repos/dnncha/turbo-picard/releases/123/assets{?name,label}",
            "assets": assets or [],
        }

    def test_uploads_only_to_matching_published_release(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assets = self.make_release_assets(root)
            requests = []

            def opener(request, timeout):
                requests.append(request)
                if request.get_method() == "GET":
                    return FakeResponse(self.release_payload())
                name = request.full_url.split("?name=", 1)[1]
                name = name.replace("%2E", ".")
                data = request.data or b""
                return FakeResponse(
                    {
                        "name": name,
                        "size": len(data),
                        "digest": f"sha256:{hashlib.sha256(data).hexdigest()}",
                    }
                )

            uploaded = upload.upload_release_assets(
                assets,
                repository="dnncha/turbo-picard",
                tag="v0.1.14",
                token="test-token",
                root=root,
                opener=opener,
            )
            self.assertEqual(len(uploaded), 8)
            self.assertEqual(requests[0].get_method(), "GET")
            self.assertTrue(all(request.headers["Authorization"] == "Bearer test-token" for request in requests))
            self.assertTrue(all(request.full_url.startswith("https://uploads.github.com/") for request in requests[1:]))

    def test_identical_assets_make_reruns_idempotent(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assets = self.make_release_assets(root)
            existing = [
                {
                    "name": path.name,
                    "size": path.stat().st_size,
                    "digest": f"sha256:{upload.asset_digest(path)}",
                }
                for path in assets.iterdir()
            ]
            calls = []

            def opener(request, timeout):
                calls.append(request)
                return FakeResponse(self.release_payload(existing))

            result = upload.upload_release_assets(
                assets,
                repository="dnncha/turbo-picard",
                tag="v0.1.14",
                token="test-token",
                root=root,
                opener=opener,
            )
            self.assertEqual(result, [])
            self.assertEqual(len(calls), 1)

    def test_refuses_to_replace_a_different_asset_before_upload(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assets = self.make_release_assets(root)
            first = next(assets.iterdir())
            existing = [{"name": first.name, "size": first.stat().st_size + 1, "digest": "sha256:" + "0" * 64}]
            calls = []

            def opener(request, timeout):
                calls.append(request)
                return FakeResponse(self.release_payload(existing))

            with self.assertRaisesRegex(RuntimeError, "different asset"):
                upload.upload_release_assets(
                    assets,
                    repository="dnncha/turbo-picard",
                    tag="v0.1.14",
                    token="test-token",
                    root=root,
                    opener=opener,
                )
            self.assertEqual(len(calls), 1)

    def test_rejects_tag_that_does_not_match_workspace_version(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assets = self.make_release_assets(root)
            with self.assertRaisesRegex(ValueError, "must match workspace version tag"):
                upload.upload_release_assets(
                    assets,
                    repository="dnncha/turbo-picard",
                    tag="v0.1.13",
                    token="test-token",
                    root=root,
                    opener=lambda *_args, **_kwargs: self.fail("API must not be called"),
                )


if __name__ == "__main__":
    unittest.main()
