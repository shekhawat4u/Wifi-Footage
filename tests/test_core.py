import json
import unittest
from pathlib import Path

from app.models import CSIFrame
from app.parser import ParseError, parse_csi_line
from app.processor import RobustBaseline, RoomProcessor, frame_features


class ParserTests(unittest.TestCase):
    def test_compact_record(self):
        frame = parse_csi_line("CSI_DATA,receiver-2,123,aa:bb,-51,-96,6,4,1,-2,3,-4")
        self.assertEqual(frame.receiver_id, "receiver-2")
        self.assertEqual(frame.iq, [1, -2, 3, -4])

    def test_rejects_short_iq(self):
        with self.assertRaises(ParseError):
            parse_csi_line("CSI_DATA,r1,123,aa,-50,-96,6,8,1,2")


class ProcessorTests(unittest.TestCase):
    def test_robust_baseline_detects_change(self):
        baseline = RobustBaseline(5)
        for _ in range(5):
            baseline.add([10.0, 20.0])
        self.assertTrue(baseline.ready)
        self.assertGreater(baseline.distance([20.0, 20.0]), 5)

    def test_processor_calibrates(self):
        config = {"width_m": 4, "height_m": 3, "calibration_samples": 3,
                  "motion_threshold": 2.8, "links": [{"id": "r1", "x": 0, "y": 0}]}
        p = RoomProcessor(config)
        for n in range(3):
            state = p.ingest(CSIFrame("r1", n, "aa", -50, -96, 6, [10, 10] * 16))
        self.assertFalse(state["links"][0]["calibrated"] is False)
        self.assertEqual(len(frame_features(CSIFrame("r1", 0, "aa", -50, -96, 6, []))), 13)


if __name__ == "__main__":
    unittest.main()

