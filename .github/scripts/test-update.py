"""PLAT-4, SEC-7, D12: deterministic tests of the local updater proof."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("update_test", Path(__file__).with_name("update_test.py"))
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


class UpdateTests(unittest.TestCase):
    def test_plat_4_requires_installed_and_new_start_in_order(self):
        start = "gitcroc 0.1.1 started (linux x86_64, AppImage)"
        self.assertFalse(driver.installed("update: installed 0.1.1\n", "0.1.1", start))
        self.assertFalse(driver.installed(start + "\nupdate: installed 0.1.1", "0.1.1", start))
        self.assertTrue(driver.installed("update: installed 0.1.1\n" + start, "0.1.1", start))

    def test_sec_7_refusal_requires_signature_error_and_unchanged_binary(self):
        self.assertFalse(driver.refused("update: refused (connection failed)", b"old", b"old"))
        self.assertFalse(driver.refused("update: refused (signature mismatch)", b"old", b"new"))
        self.assertTrue(driver.refused("update: refused (signature mismatch)", b"old", b"old"))

    def test_d12_manifest_urls_are_loopback_only(self):
        value = {"platforms": {"linux-x86_64": {"url": "https://example.test/pkg", "signature": "sig"}}}
        driver.localize(value, "http://127.0.0.1:8765")
        self.assertEqual(value["platforms"]["linux-x86_64"]["url"], "http://127.0.0.1:8765/pkg")
        self.assertEqual(value["platforms"]["linux-x86_64"]["signature"], "sig")

    def test_plat_4_waits_for_outcome_with_a_simulated_clock(self):
        with tempfile.TemporaryDirectory() as folder:
            log = Path(folder) / "app.log"
            with patch.object(driver.time, "monotonic", side_effect=[0, 1, 301]), \
                 patch.object(driver.time, "sleep"):
                with self.assertRaises(TimeoutError):
                    driver.wait(log, lambda text: "done" in text)
            with patch.object(driver.time, "monotonic", return_value=0), \
                 patch.object(driver.time, "sleep", side_effect=lambda _: log.write_text("done")):
                driver.wait(log, lambda text: "done" in text)


if __name__ == "__main__":
    unittest.main()
