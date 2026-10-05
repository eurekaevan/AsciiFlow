import os
from pathlib import Path
import tempfile
import unittest
from stack_environment import environment, loader_modules


class StackEnvironmentControls(unittest.TestCase):
    def test_environment_is_child_local_and_removes_unrecorded_selectors(self):
        base = {"PATH": "/usr/bin", "LD_LIBRARY_PATH": "/wrong", "VK_DRIVER_FILES": "/wrong", "LD_PRELOAD": "/wrong"}
        snapshot = dict(base)
        self.assertEqual(environment(base=base), {"PATH": "/usr/bin"})
        self.assertEqual(base, snapshot)

    def test_explicit_prefix_and_selectors_are_resolved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            icd = root / "intel.json"
            icd.touch()
            env = environment(root, {"VK_ICD_FILENAMES": str(icd), "LIBVA_DRIVER_NAME": "iHD"}, {"PATH": "/usr/bin"})
            self.assertEqual(env["PATH"], str(root / "bin") + os.pathsep + "/usr/bin")
            self.assertEqual(env["LD_LIBRARY_PATH"], str(root / "lib"))
            self.assertEqual(env["VK_ICD_FILENAMES"], str(icd))

    def test_unknown_missing_and_multi_driver_selectors_fail_closed(self):
        for selectors in [{"PATH": "/tmp"}, {"LIBVA_DRIVER_NAME": "fake"}, {"LIBVA_DRIVERS_PATH": "a:b"}, {"VK_ICD_FILENAMES": "/does-not-exist"}]:
            with self.subTest(selectors=selectors), self.assertRaises((ValueError, FileNotFoundError)):
                environment(selectors=selectors, base={})

    def test_requested_search_paths_are_not_loaded_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "driver.so"
            file.touch()
            self.assertEqual(loader_modules(f"trying file={file}\n"), [])
            self.assertEqual(loader_modules(f"123: calling init: {file}\n123: calling init: {file}\n"), [str(file)])


if __name__ == "__main__":
    unittest.main()
