"""Negative fixtures: prose regressions, proof drift, and internal-symbol immunity."""
import sys
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from claims_policy import violations
from claims_sources import audit

class PublicClaimsTests(unittest.TestCase):
    def test_unsupported_claims_fail(self):
        claims = [
            'Without `DARIUS_API_KEY`, `darius run` uses the offline `MockModel`.',
            'Clean terminal exits under 2 seconds.',
            'Guaranteed restoration upon any exit or signal.',
            'Live MCP client and subagent orchestration.',
            'A2A hub with peer_send and cron scheduling.',
            'Worktree rollback protects every edit.',
            'darius approval-check validates permissions.',
        ]
        for claim in claims:
            with self.subTest(claim=claim):
                self.assertTrue(violations(claim))

    def test_retirement_and_explicit_demo_pass(self):
        self.assertEqual(violations('MCP, subagent, A2A, cron and rollback are unavailable.'), [])
        self.assertEqual(violations('Explicit --offline selects MockModel; missing keys are errors.'), [])

    def test_proof_drift_and_internal_symbols(self):
        import tempfile
        from shutil import copytree, copy2
        root = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory() as directory:
            fixture = Path(directory)
            for name in ('scripts', 'docs', 'crates', '.github'):
                copytree(root / name, fixture / name)
            for name in ('README.md', 'CHANGELOG.md', 'Cargo.toml', 'install.sh'):
                copy2(root / name, fixture / name)
            self.assertEqual(audit(fixture), [])
            internal = fixture / 'crates/darius-core/src/private_claim.rs'
            internal.write_text('fn peer_send() {} // MCP A2A cron rollback subagent')
            self.assertEqual(audit(fixture), [])
            for name in ('Cargo.toml', '.github/workflows/release.yml'):
                target = fixture / name
                before = target.read_text()
                target.write_text(before + '\ndescription = "Most powerful A2A hub"\n')
                self.assertTrue(audit(fixture), name)
                target.write_text(before)
            doc = fixture / 'docs/CAPABILITIES.md'
            doc.write_text(doc.read_text() + '\n| **Verified** | [`imaginary_test`](../crates/darius-core/src/commands.rs) |\n')
            self.assertTrue(any('missing proof' in error for error in audit(fixture)))
            (fixture / 'README.md').write_text('Without API key, run uses MockModel')
            self.assertTrue(any('silent mock' in error for error in audit(fixture)))

if __name__ == '__main__':
    unittest.main()
