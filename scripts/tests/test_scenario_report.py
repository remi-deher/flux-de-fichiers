"""Vérifie le rapport sans démarrer ni modifier un laboratoire Docker."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import patch


class ScenarioReportTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        secrets = self.root / "var/secrets"
        secrets.mkdir(parents=True)
        for name in ("admin_key", "worker_key"):
            (secrets / name).write_text("test-placeholder")
        spec = importlib.util.spec_from_file_location(
            "scenario_report_test", Path(__file__).resolve().parents[1] / "scenarios.py")
        self.scenarios = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, {"dev": types.SimpleNamespace(ROOT=self.root)}), \
                patch("ssl.create_default_context"):
            spec.loader.exec_module(self.scenarios)

    def check_report(self, status):
        report = json.loads((self.root / "var/scenario-report.json").read_text())
        self.assertEqual(report["status"], status)
        self.assertEqual(report["passed"], ["TLS"])
        self.assertEqual(report["duration_seconds"], 2.5)
        self.assertNotIn("sensitive-detail", json.dumps(report))

    def test_success_records_progress_and_duration(self):
        self.scenarios.RESULTS.append("TLS")
        with patch.object(self.scenarios, "run"), \
                patch.object(self.scenarios.time, "monotonic", side_effect=[10, 12.5]):
            self.scenarios.main()
        self.check_report("passed")

    def test_failure_preserves_report_and_propagates_exception(self):
        self.scenarios.RESULTS.append("TLS")
        with patch.object(self.scenarios, "run", side_effect=AssertionError("sensitive-detail")), \
                patch.object(self.scenarios.time, "monotonic", side_effect=[10, 12.5]):
            with self.assertRaises(AssertionError):
                self.scenarios.main()
        self.check_report("failed")


if __name__ == "__main__":
    unittest.main()
