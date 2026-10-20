"""D12, PLAT-3, PLAT-4: deterministic tests of the installed-app outcome checker."""

import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

from install_common import wait_started


class InstallTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.log = Path(self.directory.name) / "gitcroc.log"
        self.process = Mock()
        self.process.poll.return_value = None
        self.start = "2026-10-20T00:00:00Z gitcroc 0.1.0 started (linux x86_64, deb)\n"
        self.git = '2026-10-20T00:00:00Z git "--version" in the app\'s directory, 1 ms, exit 0\n'

    def test_plat_3_plat_4_requires_start_and_successful_git(self):
        self.log.write_text(self.start + self.git)
        wait_started(self.process, self.log, "0.1.0", "linux", "x86_64", "deb")

    def test_plat_3_missing_or_failed_git_does_not_pass(self):
        for text in (self.start, self.start + self.git.replace("exit 0", "exit 1")):
            with self.subTest(text=text):
                self.log.write_text(text)
                with patch("install_common.time.monotonic", side_effect=[0, 301]):
                    with self.assertRaises(TimeoutError):
                        wait_started(self.process, self.log, "0.1.0", "linux", "x86_64", "deb")

    def test_plat_4_wrong_version_platform_or_package_does_not_pass(self):
        for old, new in (("0.1.0", "0.2.0"), ("linux", "macos"), ("x86_64", "aarch64"), ("deb", "rpm")):
            self.log.write_text(self.start.replace(old, new) + self.git)
            with patch("install_common.time.monotonic", side_effect=[0, 301]):
                with self.assertRaises(TimeoutError):
                    wait_started(self.process, self.log, "0.1.0", "linux", "x86_64", "deb")

    def test_plat_4_exit_even_with_good_log_fails(self):
        self.log.write_text(self.start + self.git)
        self.process.poll.return_value = 1
        with self.assertRaises(RuntimeError):
            wait_started(self.process, self.log, "0.1.0", "linux", "x86_64", "deb")

    def test_plat_3_waits_for_the_outcome(self):
        self.log.write_text(self.start)
        def git_arrives(_):
            self.log.write_text(self.start + self.git)
        with patch("install_common.time.sleep", side_effect=git_arrives):
            wait_started(self.process, self.log, "0.1.0", "linux", "x86_64", "deb")


if __name__ == "__main__":
    unittest.main()
