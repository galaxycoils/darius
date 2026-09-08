"""Release evidence must bind real tagged history, not caller-supplied labels."""
import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

class ProvenanceTests(unittest.TestCase):
    def test_tag_history_and_dirty_tree_fail_closed(self):
        spec = importlib.util.spec_from_file_location("provenance", ROOT / "scripts/release_provenance.py")
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            def git(*args):
                return subprocess.check_output(["git", "-C", directory, *args], stderr=subprocess.DEVNULL, text=True).strip()
            git("init")
            git("config", "user.email", "fixture@example.invalid")
            git("config", "user.name", "Fixture")
            tracked = repo / "tracked"
            tracked.write_text("baseline")
            git("add", "tracked")
            git("commit", "-m", "baseline")
            baseline = git("rev-parse", "HEAD")
            tracked.write_text("release")
            git("commit", "-am", "release")
            head = git("rev-parse", "HEAD")
            git("tag", "v1.2.0")
            self.assertEqual(module.provenance(repo, "v1.2.0", baseline, []), (head, [head]))
            for tag, base, work in [("v9.9.9", baseline, []), ("v1.2.0", "abc123", []),
                                    ("v1.2.0", baseline, [baseline])]:
                with self.assertRaises(ValueError):
                    module.provenance(repo, tag, base, work)
            tracked.write_text("dirty")
            with self.assertRaises(ValueError):
                module.provenance(repo, "v1.2.0", baseline, [])
            git("commit", "-am", "after tag")
            with self.assertRaises(ValueError):
                module.provenance(repo, "v1.2.0", baseline, [])

if __name__ == "__main__":
    unittest.main()
