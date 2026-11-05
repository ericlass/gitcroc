"""PLAT-4, SEC-7, D12 (M6-28c): deterministic tests of the update test's
helpers, without starting an app."""

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import update_rc1_build
import update_test
import update_version


class VersionTests(unittest.TestCase):
    def test_plat_4_b_raises_the_patch_number(self):
        self.assertEqual(update_version.newer("1.0.0-rc.1"), "1.0.1-rc.1")
        self.assertEqual(update_version.newer("1.2.9"), "1.2.10")
        self.assertEqual(update_version.newer("1.0.0+build.5"), "1.0.1+build.5")
        with self.assertRaises(ValueError):
            update_version.newer("1.0")

    def test_plat_4_bump_sets_the_workspace_and_its_lock_entries(self):
        manifest = '[workspace]\nmembers = []\n\n[workspace.package]\nversion = "1.0.0"\nedition = "2024"\n'
        self.assertEqual(update_version.manifest_version(manifest), "1.0.0")
        self.assertEqual(update_version.workspace_version(), update_version.manifest_version(
            (update_version.ROOT / "Cargo.toml").read_text(encoding="utf-8")))
        self.assertIn('version = "1.0.1"', update_version.bump_manifest(manifest, "1.0.0", "1.0.1"))
        lock = (
            '# lock\nversion = 4\n\n[[package]]\nname = "gitcroc-app"\nversion = "1.0.0"\n'
            '\n[[package]]\nname = "dep"\nversion = "1.0.0"\nsource = "registry+https://example"\n'
        )
        bumped = update_version.bump_lock(lock, "1.0.0", "1.0.1")
        self.assertIn('name = "gitcroc-app"\nversion = "1.0.1"', bumped)
        # A dependency from a registry keeps its version.
        self.assertIn('name = "dep"\nversion = "1.0.0"', bumped)
        with self.assertRaises(ValueError):
            update_version.bump_lock(lock, "2.0.0", "2.0.1")


class OutcomeTests(unittest.TestCase):
    def test_plat_4_installed_needs_the_line_and_the_new_start(self):
        start = update_test.start_line("1.0.1")
        self.assertFalse(update_test.installed("update: installed 1.0.1\n", "1.0.1"))
        self.assertFalse(update_test.installed(start + "\n", "1.0.1"))
        self.assertTrue(update_test.installed("warning: update: installed 1.0.1\n" + start, "1.0.1"))
        # The Windows installer's new version logs both, its start first.
        self.assertTrue(update_test.installed(start + "\nwarning: update: installed 1.0.1", "1.0.1"))

    def test_sec_7_refused_needs_a_signature_reason(self):
        self.assertFalse(update_test.refused("update: refused (connection refused)"))
        self.assertTrue(update_test.refused("update: refused (the update's signature does not match: x)"))
        self.assertTrue(update_test.refused("update: refused (Signature mismatch)"))

    def test_d12_manifest_urls_are_loopback_only(self):
        value = {"platforms": {"linux-x86_64": {"url": "https://github.com/x/releases/download/v1/pkg", "signature": "s"}}}
        update_test.localize(value)
        self.assertEqual(value["platforms"]["linux-x86_64"]["url"], "http://127.0.0.1:8765/pkg")
        self.assertEqual(value["platforms"]["linux-x86_64"]["signature"], "s")
        with self.assertRaises(ValueError):
            update_test.localize(value, "http://example.com:8765")

    def test_d12_rc1_points_only_at_the_local_server_with_the_test_key(self):
        value = update_rc1_build.overlay("1.0.0-rc.1", "KEY")
        self.assertEqual(value["version"], "1.0.0-rc.1")
        self.assertEqual(value["plugins"]["updater"]["endpoints"], ["http://127.0.0.1:8765/latest.json"])
        self.assertEqual(value["plugins"]["updater"]["pubkey"], "KEY")
        self.assertFalse(value["bundle"]["createUpdaterArtifacts"])

    def test_plat_4_waits_for_the_line_with_a_simulated_clock(self):
        with tempfile.TemporaryDirectory() as folder:
            log = Path(folder) / "gitcroc.log"
            with patch.object(update_test.time, "monotonic", side_effect=[0, 1, update_test.DEADLINE + 1]), \
                 patch.object(update_test.time, "sleep"):
                with self.assertRaises(TimeoutError):
                    update_test.wait(log, lambda text: "done" in text, "done")
            with patch.object(update_test.time, "monotonic", return_value=0), \
                 patch.object(update_test.time, "sleep", side_effect=lambda _: log.write_text("done")):
                self.assertEqual(update_test.wait(log, lambda text: "done" in text, "done"), "done")

    def test_sec_7_outputs_stay_outside_the_cargo_target(self):
        self.assertFalse(update_test.WORK.is_relative_to(update_test.ROOT / "target"))


if __name__ == "__main__":
    unittest.main()
