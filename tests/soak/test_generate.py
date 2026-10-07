"""Safety checks; actual fixed-tool materialization is an explicit preflight."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from generate import generate


class GeneratorSafety(unittest.TestCase):
    def test_invalid_frame_count_does_not_create_output_or_start_tools(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary) / "new"
            with patch("generate.subprocess.run") as tool:
                for frames in (0, -1):
                    with self.assertRaises(ValueError):
                        generate("sdr", frames, directory)
                tool.assert_not_called()
            self.assertFalse(directory.exists())

    def test_existing_evidence_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            sentinel = directory / "input.mp4"
            sentinel.write_bytes(b"preserved evidence")
            with patch("generate.subprocess.run") as tool:
                with self.assertRaises(FileExistsError):
                    generate("sdr", 1000, directory)
                tool.assert_not_called()
            self.assertEqual(sentinel.read_bytes(), b"preserved evidence")


if __name__ == "__main__":
    unittest.main()
