"""QA-3: documentation screenshots preserve Xvfb pixels and row padding."""
from pathlib import Path
import struct
import tempfile
import unittest
from PIL import Image
from capture_smoke import png


class Capture(unittest.TestCase):
    def test_qa_3_xwd_24_and_32_bit_pixels_keep_colors_and_row_padding(self):
        for bpp, pixels in [(24, bytes([0, 0, 255, 0, 255, 0, 0, 0])),
                            (32, bytes([0, 0, 255, 0, 0, 255, 0, 0]))]:
            header = [100, 7, 2, 24, 2, 1, 0, 0, 32, 0, 8, bpp, 8,
                      5, 0xFF0000, 0xFF00, 0xFF, 8, 256, 0, 2, 1, 0, 0, 0]
            with tempfile.TemporaryDirectory() as directory:
                raw, image = Path(directory) / "capture.xwd", Path(directory) / "capture.png"
                raw.write_bytes(struct.pack(">25I", *header) + pixels)
                png(raw, image)
                with Image.open(image) as result:
                    self.assertEqual(list(result.getdata()), [(255, 0, 0), (0, 255, 0)])


if __name__ == "__main__":
    unittest.main()
