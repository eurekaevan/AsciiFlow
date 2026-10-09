import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("ab_closure", Path(__file__).with_name("ab-closure.py"))
closure = importlib.util.module_from_spec(spec)
spec.loader.exec_module(closure)


class EncoderCaptureValidation(unittest.TestCase):
    def fixture(self, root, rows):
        (root / "frames.jsonl").write_text("\n".join(json.dumps(row) for row in rows))
        (root / "frames.nv12").write_bytes(bytes(128 * 96 * 3 // 2 * len(rows)))
        for phase in ("before", "after"):
            (root / f"context-{phase}.json").write_text("{}")
            (root / f"avoptions-{phase}.txt").write_text("fixed options")

    def test_actual_non1080_geometry_is_validated(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rows = [{"index": 0, "width": 128, "height": 96, "software_format": 23}]
            self.fixture(root, rows)
            self.assertEqual(closure.capture_record(root)["raw_nv12_bytes"], 18432)

    def test_empty_or_unordered_capture_rejected(self):
        for rows in ([], [{"index": 1, "width": 128, "height": 96, "software_format": 23}]):
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                self.fixture(root, rows)
                with self.assertRaises(ValueError):
                    closure.capture_record(root)

    def test_format_and_raw_length_rejected(self):
        for format_code, truncate in ((0, False), (23, True)):
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                self.fixture(root, [{"index": 0, "width": 128, "height": 96,
                                     "software_format": format_code}])
                if truncate:
                    (root / "frames.nv12").write_bytes(b"short")
                with self.assertRaises(ValueError):
                    closure.capture_record(root)
