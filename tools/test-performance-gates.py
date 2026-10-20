#!/usr/bin/env python3
"""Exercise the D9 runner's failure paths without building the shell."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent


class WorkloadGates(unittest.TestCase):
    def run_gate(self, case="pass", baseline=None):
        with tempfile.TemporaryDirectory() as directory:
            cargo = Path(directory) / "cargo"
            cargo.write_text('''#!/usr/bin/env python3
import os
import sys
import json
if "--no-run" in sys.argv:
    if os.environ["GATE_CASE"] == "build-failure":
        sys.exit(1)
    if os.environ["GATE_CASE"] == "missing-artifact":
        sys.exit(0)
    print(json.dumps({"reason": "compiler-artifact", "target": {"name": "mesh_core_shell"}, "profile": {"test": True}, "executable": os.path.abspath(sys.argv[0])}))
    sys.exit(0)
names = {
    "d9_navigation_updates": [("hover_p90_ms", 1), ("service_update_p90_ms", 1)],
    "d9_popover_open": [("popover_open_p90_ms", 1)],
    "d9_theme_switch": [("theme_switch_p90_ms", 1)],
    "d9_coalesced_reload_wakeup_profile": [("idle_wakeups_per_minute", 30)],
    "d9_settings_scroll_frame_gate": [("scroll_p90_ms", 1)],
}
case = os.environ["GATE_CASE"]
if case == "test-failure":
    sys.exit(1)
for argument in sys.argv:
    for metric, value in names.get(argument, []):
        if metric == "hover_p90_ms":
            if case == "missing":
                continue
            if case == "over-budget":
                value = 1000
            if case == "invalid":
                value = "NaN"
        line = f"MESH_PERF metric=d9_{metric} value={value}"
        print(("test workload ... " if case == "harness-prefix" else "") + line)
        if case == "duplicate" and metric == "hover_p90_ms":
            print(line)
''')
            cargo.chmod(0o755)
            environment = dict(os.environ, PATH=directory + os.pathsep + os.environ["PATH"], GATE_CASE=case)
            environment["MESH_PERF_BASELINE"] = str(ROOT / "config/performance-baseline.tsv")
            if baseline is not None:
                path = Path(directory) / "baseline.tsv"
                path.write_text(baseline)
                environment["MESH_PERF_BASELINE"] = str(path)
            return subprocess.run(
                [str(ROOT / "tools/check-performance"), "--workloads-only"],
                env=environment, capture_output=True, text=True, check=False,
            )

    def test_complete_measurements_pass(self):
        result = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout.count(" PASS"), 6)

    def test_harness_prefix_preserves_measurements(self):
        result = self.run_gate("harness-prefix")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bad_measurements_fail(self):
        for case in ["missing", "duplicate", "invalid", "over-budget", "test-failure", "build-failure", "missing-artifact"]:
            with self.subTest(case=case):
                self.assertNotEqual(self.run_gate(case).returncode, 0)

    def test_missing_budget_fails(self):
        self.assertNotEqual(self.run_gate(baseline="# no budgets\n").returncode, 0)


class GpuWorkloadGates(unittest.TestCase):
    def run_gate(self, case):
        with tempfile.TemporaryDirectory() as directory:
            cargo = Path(directory) / "cargo"
            cargo.write_text('''#!/usr/bin/env python3
import json
import os
import sys
case = os.environ["GATE_CASE"]
assert os.environ.get("MESH_REQUIRE_GPU") == "1"
if "--no-run" in sys.argv:
    if case == "build-failure":
        sys.exit(1)
    if case != "missing-artifact":
        print(json.dumps({"reason": "compiler-artifact", "target": {"name": "mesh_core_render"}, "profile": {"test": True}, "executable": os.path.abspath(sys.argv[0])}))
    sys.exit(0)
if case == "test-failure":
    sys.exit(1)
assert "--exact" in sys.argv and "--ignored" in sys.argv
assert any(name in sys.argv for name in ["gpu::tests::frame_cost::gpu_workload_visual_match_gate", "gpu::tests::frame_cost::gpu_workload_frame_cost_gate"])
print("test result: ok. " + ("0" if case == "zero-tests" else "1") + " passed; 0 failed;")
''')
            cargo.chmod(0o755)
            environment = dict(os.environ, PATH=directory + os.pathsep + os.environ["PATH"], GATE_CASE=case)
            return subprocess.run([str(ROOT / "tools/check-gpu-performance")], env=environment, capture_output=True, text=True, check=False)

    def test_required_gpu_gates_execute(self):
        result = self.run_gate("pass")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout.count("1 passed;"), 2)

    def test_missing_or_failed_gates_fail(self):
        for case in ["build-failure", "missing-artifact", "test-failure", "zero-tests"]:
            with self.subTest(case=case):
                self.assertNotEqual(self.run_gate(case).returncode, 0)


if __name__ == "__main__":
    unittest.main()
