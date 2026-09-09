"""Synthetic output fixtures prove generated-surface failures, not runtime success."""
import sys
import unittest
from pathlib import Path
from subprocess import CompletedProcess
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from claims_runtime import audit

class GeneratedClaimsTests(unittest.TestCase):
    def result(self, command, **kwargs):
        key = tuple(command[1:])
        outputs = {
            ('--help',): 'Commands:\n  tui \n  run \n  config \n  memory \n  serve \n\nOptions:',
            ('config', '--help'): 'Commands:\n  show \n  init \n  preset \n\nOptions:',
            ('memory', '--help'): 'Commands:\n  search \n  pack \n  import \n  export \n  stats \n\nOptions:',
            ('config', 'show'): 'Runtime state: setup\nMemory: unavailable\nProvider URL: not configured',
        }
        return CompletedProcess(command, 0, self.overrides.get(key, outputs.get(key, 'Usage: fixture')), '')

    def check(self, overrides):
        self.overrides = overrides
        with patch('claims_runtime.subprocess.run', side_effect=self.result):
            return audit(Path.cwd(), 'fixture-binary')

    def test_clean_fixture(self):
        self.assertEqual(self.check({}), [])

    def test_nested_help_and_diagnostics_fail(self):
        cases = [
            {('config', 'init', '--help'): 'Live MCP support'},
            {('memory', '--help'): 'Commands:\n  cron \n\nOptions:'},
            {('config', 'show'): 'Runtime state: live\nMemory: open'},
            {('install.sh', '--help'): 'Most powerful A2A server'},
        ]
        for case in cases:
            with self.subTest(case=case):
                self.assertTrue(self.check(case))

if __name__ == '__main__':
    unittest.main()
