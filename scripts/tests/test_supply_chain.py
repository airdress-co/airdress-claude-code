"""Tests for the release's two pinning steps: the reproducible bundle and
the marketplace pin. Run: python3 -m unittest discover -s scripts/tests
"""

from __future__ import annotations

import importlib.util
import json
import os
import pathlib
import subprocess
import tempfile
import time
import unittest
import zipfile

SCRIPTS = pathlib.Path(__file__).resolve().parent.parent


def load(name: str):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), SCRIPTS / f"{name}.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


pkg = load("package-mcpb")
pins_mod = load("marketplace-pins")


class Bundle(unittest.TestCase):
    def build(self, root: pathlib.Path, label: str, mtime: float) -> bytes:
        d = root / label
        d.mkdir()
        server = d / "airdress-mcp"
        server.write_bytes(b"\x7fELF pretend server")
        lic = d / "LICENSE"
        lic.write_text("Apache-2.0\n")
        for f in (server, lic):
            os.utime(f, (mtime, mtime))
        os.chmod(server, 0o700 if label == "a" else 0o775)
        out = d / "out.mcpb"
        pkg.package(
            platform="linux-x86_64",
            server=server,
            version="0.1.0",
            out=out,
            epoch=1759000000,
            extra={"LICENSE": lic.read_bytes()},
        )
        return out.read_bytes()

    def test_two_builds_with_different_paths_times_and_modes_are_identical(self):
        with tempfile.TemporaryDirectory() as t:
            root = pathlib.Path(t)
            a = self.build(root, "a", time.time())
            b = self.build(root, "b", 1_000_000_000)
            self.assertEqual(a, b)

    def test_layout_the_launcher_reads(self):
        with tempfile.TemporaryDirectory() as t:
            root = pathlib.Path(t)
            self.build(root, "a", time.time())
            with zipfile.ZipFile(root / "a" / "out.mcpb") as zf:
                names = zf.namelist()
                self.assertEqual(names, sorted(names))
                self.assertIn("airdress-mcp", names)
                info = zf.getinfo("airdress-mcp")
                self.assertEqual(info.compress_type, zipfile.ZIP_STORED)
                self.assertEqual((info.external_attr >> 16) & 0o777, 0o755)
                m = json.loads(zf.read("manifest.json"))
                self.assertEqual(m["manifest_version"], "0.3")
                self.assertEqual(m["server"]["type"], "binary")
                self.assertEqual(m["server"]["entry_point"], "airdress-mcp")
                self.assertEqual(m["version"], "0.1.0")

    def test_refuses_an_unknown_platform(self):
        with tempfile.TemporaryDirectory() as t:
            s = pathlib.Path(t) / "s"
            s.write_bytes(b"x")
            with self.assertRaises(SystemExit):
                pkg.package(platform="windows", server=s, version="1", out=pathlib.Path(t) / "o",
                            epoch=0, extra={})


def pins_doc(version: str, sha: str = "a" * 64) -> dict:
    tag = f"v{version}"
    ident = f"https://github.com/airdress-co/airdress-claude-code/.github/workflows/release.yml@refs/tags/{tag}"
    plats = {}
    for p in ("linux-x86_64", "linux-aarch64", "darwin-universal"):
        art = f"airdress-{p}.mcpb"
        plats[p] = {
            "sha256": sha,
            "bundle": {
                "cdn": f"https://downloads.airdress.co/claude-code/{tag}/{art}",
                "github": f"https://github.com/airdress-co/airdress-claude-code/releases/download/{tag}/{art}",
            },
            "sigstore": {
                "cdn": f"https://downloads.airdress.co/claude-code/{tag}/{art}.sigstore.json",
                "github": f"https://github.com/airdress-co/airdress-claude-code/releases/download/{tag}/{art}.sigstore.json",
            },
            "cert_identity": ident,
            "cert_issuer": "https://token.actions.githubusercontent.com",
        }
    return {"version": version, "platforms": plats}


class MarketplacePins(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.dir.name)
        self.cwd = os.getcwd()
        os.chdir(self.root)
        subprocess.run(["git", "init", "-q", "-b", "main"], check=True)
        subprocess.run(["git", "config", "user.email", "t@example.invalid"], check=True)
        subprocess.run(["git", "config", "user.name", "t"], check=True)
        (self.root / ".claude-plugin").mkdir()
        (self.root / "plugins/airdress/.claude-plugin").mkdir(parents=True)
        (self.root / "plugins/airdress/launcher").mkdir()
        (self.root / "plugins/airdress/.claude-plugin/plugin.json").write_text(
            json.dumps({"name": "airdress", "version": "0.1.0"}))
        self.market = self.root / ".claude-plugin/marketplace.json"
        self.market.write_text(json.dumps(
            {"name": "airdress", "plugins": [{"name": "airdress", "source": "./plugins/airdress"}]}))
        self.commit("pre-release")

    def tearDown(self):
        os.chdir(self.cwd)
        self.dir.cleanup()

    def commit(self, msg: str) -> str:
        subprocess.run(["git", "add", "-A"], check=True)
        subprocess.run(["git", "commit", "-q", "-m", msg], check=True)
        return subprocess.run(["git", "rev-parse", "HEAD"], check=True, capture_output=True,
                              text=True).stdout.strip()

    def test_relative_source_is_allowed_only_before_a_release(self):
        self.assertEqual(pins_mod.check(), 0)
        (self.root / pins_mod.PINS).write_text(json.dumps(pins_doc("0.1.0")))
        self.commit("pins")
        self.assertEqual(pins_mod.check(), 1)

    def test_pin_then_check_passes_and_tampering_fails(self):
        (self.root / pins_mod.PINS).write_text(json.dumps(pins_doc("0.1.0")))
        sha = self.commit("pins")
        self.assertEqual(pins_mod.pin(sha), 0)
        entry = json.loads(self.market.read_text())["plugins"][0]
        self.assertEqual(entry["source"]["sha"], sha)
        self.assertEqual(entry["source"]["source"], "git-subdir")
        self.assertEqual(entry["source"]["path"], "plugins/airdress")
        self.assertEqual(pins_mod.check(), 0)

        doc = json.loads(self.market.read_text())
        doc["plugins"][0]["metadata"]["bundles_sha256"]["linux-x86_64"] = "b" * 64
        self.market.write_text(json.dumps(doc))
        self.assertEqual(pins_mod.check(), 1)

    def test_short_or_unknown_commit_fails(self):
        (self.root / pins_mod.PINS).write_text(json.dumps(pins_doc("0.1.0")))
        sha = self.commit("pins")
        pins_mod.pin(sha)
        for bad in (sha[:12], "c" * 40):
            doc = json.loads(self.market.read_text())
            doc["plugins"][0]["source"]["sha"] = bad
            self.market.write_text(json.dumps(doc))
            self.assertEqual(pins_mod.check(), 1, bad)

    def test_pins_for_another_version_or_identity_fail(self):
        doc = pins_doc("0.2.0")
        self.assertTrue(pins_mod.check_pins(doc, "0.1.0"))
        doc = pins_doc("0.1.0")
        doc["platforms"]["linux-x86_64"]["cert_identity"] += "x"
        self.assertTrue(pins_mod.check_pins(doc, "0.1.0"))
        doc = pins_doc("0.1.0")
        doc["platforms"]["darwin-universal"]["bundle"]["cdn"] = "https://evil.example/x"
        self.assertTrue(pins_mod.check_pins(doc, "0.1.0"))
        self.assertEqual(pins_mod.check_pins(pins_doc("0.1.0"), "0.1.0"), [])


if __name__ == "__main__":
    unittest.main()
