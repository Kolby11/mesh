#!/usr/bin/env python3
"""Regression checks for session node counts and stage timing separation."""
import collections
import pathlib
import runpy
import tempfile
import unittest

REPORT = runpy.run_path(str(pathlib.Path(__file__).with_name("profile-report")), run_name="profile_report_test")


class NodeVisitReportTests(unittest.TestCase):
    def test_counts_are_grouped_by_surface_without_becoming_timing_samples(self):
        lines = [
            '2026-10-21T12:00:00.000000Z DEBUG mesh_profiling: frame node visits surface="dock" pass="animation_sample" node_visits=2',
            '2026-10-21T12:00:00.000001Z DEBUG mesh_profiling: frame node visits surface="dock" pass="animation_sample" node_visits=4',
            '2026-10-21T12:00:00.000002Z DEBUG mesh_profiling: frame node visits surface="dashboard" pass="animation_sample" node_visits=0',
            '2026-10-21T12:00:00.000003Z DEBUG mesh_profiling: profiling sample stage="total_surface_render" duration_us=500 surface="dock" trigger="paint"',
        ]
        with tempfile.TemporaryDirectory() as directory:
            log = pathlib.Path(directory) / "activity.log"
            log.write_text("\n".join(lines) + "\n")
            _, stages, _ = REPORT["read_log"](log)
        timestamp = stages[0][0]
        segments = [{"start": timestamp - 1, "end": timestamp + 1,
                     "surfaces": collections.Counter(), "events": 0}]
        output = []
        REPORT["report_segment"](segments, [], stages, 499, 10, output, "test")
        rendered = "\n".join(output)
        self.assertIn("| dock | animation_sample | 2 | 6 | 3.0 | 4 |", rendered)
        self.assertIn("| dashboard | animation_sample | 1 | 0 | 0.0 | 0 |", rendered)
        self.assertIn("| total_surface_render [paint] | 1 |", rendered)
        self.assertIn("(1 surface renders,", rendered)
        self.assertNotIn("| ? |", rendered)


if __name__ == "__main__":
    unittest.main()
