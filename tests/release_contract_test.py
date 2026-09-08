"""Frozen release gates must fail closed, including syntax lint."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]

class ReleaseContractTests(unittest.TestCase):
    def test_mandatory_pinned_actionlint_in_ci_and_prerequisite(self):
        helper = ROOT / "scripts/release-actionlint.sh"
        self.assertTrue(helper.exists(), "missing mandatory local actionlint gate")
        text = helper.read_text()
        self.assertIn("v1.7.12", text)
        for name in ("ci.yml", "release.yml"):
            workflow = (ROOT / ".github/workflows" / name).read_text()
            self.assertIn("bash scripts/release-actionlint.sh", workflow)
            self.assertNotIn("continue-on-error: true", workflow)
        release = (ROOT / ".github/workflows/release.yml").read_text()
        self.assertIn("bash scripts/release-actionlint.sh", release.split("  build:")[0])

    def test_release_evidence_and_exact_host_binary_gate(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        self.assertIn("scripts/release-evidence.py", workflow)
        self.assertIn("bash scripts/release-host.sh", workflow)
        self.assertIn("fetch-depth: 0", workflow)
        host = (ROOT / "scripts/release-host.sh").read_text()
        self.assertIn("DARIUS_BIN_UNDER_TEST", host)
        self.assertIn("first_run_setup_journey", host)
        self.assertIn("full_agent_journey", host)
        self.assertIn('"$DARIUS_BIN_UNDER_TEST" < /dev/null\n', host)
        self.assertIn('cd "$REPO_ROOT"', host)
        evidence = (ROOT / "scripts/release-evidence.py").read_text()
        self.assertIn('provenance(repo, args.tag, args.baseline, args.wu_shas)', evidence)

if __name__ == "__main__":
    unittest.main()
