"""Installer regressions use real curl against a loopback release fixture."""
import functools
import hashlib
import http.server
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parents[1]

class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.dist = self.root / "v1.2.0"
        self.dist.mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.old = b'#!/bin/sh\necho "darius 1.0.0"\n'
        (self.bin / "darius").write_bytes(self.old)
        (self.bin / "darius").chmod(0o755)
        self.asset = subprocess.check_output(["bash", str(ROOT / "scripts/release-target.sh")], text=True).strip()
        self.env = dict(os.environ)
        self.package("darius 1.2.0")

    def package(self, output):
        src = self.root / "darius"
        src.write_text('#!/bin/sh\nprintf "%s\\n" "' + output + '"\n')
        src.chmod(0o755)
        with tarfile.open(self.dist / self.asset, "w:gz") as archive:
            archive.add(src, arcname="darius")
        digest = hashlib.sha256((self.dist / self.asset).read_bytes()).hexdigest()
        (self.dist / self.asset.replace(".tar.gz", ".sha256")).write_text(digest + "  " + self.asset + "\n")

    def run_install(self, remote=False):
        args = ["bash", str(ROOT / "install.sh"), "--version", "1.2.0", "--install-dir", str(self.bin)]
        if not remote:
            args += ["--artifact-dir", str(self.dist)]
        return subprocess.run(args, env=self.env, text=True, capture_output=True, timeout=15)

    def preserved(self):
        self.assertEqual((self.bin / "darius").read_bytes(), self.old)
        self.assertEqual(sorted(p.name for p in self.bin.iterdir()), ["darius"])

    def test_exact_version_rejects_substrings_and_extra_output(self):
        for output in ("darius 11.2.0", "darius 1.2.00", "darius 1.2.0-dev", "other 1.2.0", "darius 1.2.0\nextra"):
            with self.subTest(output=output):
                self.package(output)
                result = self.run_install()
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.preserved()

    def test_unsupported_matrix_rejected_by_helper_and_installer(self):
        mock = self.root / "mock"
        mock.mkdir()
        self.env["PATH"] = str(mock) + os.pathsep + os.environ["PATH"]
        for system, arch in (("Linux", "aarch64"), ("FreeBSD", "x86_64"), ("Darwin", "i386")):
            with self.subTest(system=system, arch=arch):
                helper = subprocess.run(["bash", str(ROOT / "scripts/release-target.sh"), system, arch], capture_output=True)
                self.assertNotEqual(helper.returncode, 0)
                uname = mock / "uname"
                uname.write_text(f'#!/bin/sh\ncase "$1" in -s) echo {system};; -m) echo {arch};; esac\n')
                uname.chmod(0o755)
                result = self.run_install()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Unsupported", result.stderr)
                self.preserved()

    def test_failed_atomic_rename_cleans_stage_and_preserves_old(self):
        mock = self.root / "mock"
        mock.mkdir()
        mv = mock / "mv"
        mv.write_text("#!/bin/sh\nexit 73\n")
        mv.chmod(0o755)
        self.env["PATH"] = str(mock) + os.pathsep + os.environ["PATH"]
        result = self.run_install()
        self.assertNotEqual(result.returncode, 0)
        self.preserved()

    def serve(self):
        class Quiet(http.server.SimpleHTTPRequestHandler):
            def log_message(self, format, *args):
                pass
        handler = functools.partial(Quiet, directory=str(self.root))
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        def close():
            server.shutdown()
            server.server_close()
            thread.join()
        self.addCleanup(close)
        self.env["DARIUS_RELEASE_BASE_URL"] = f"http://127.0.0.1:{server.server_port}"

    def test_http_success_atomic_replacement_and_restore(self):
        self.serve()
        backup = self.root / "preserved-darius"
        shutil.copy2(self.bin / "darius", backup)
        old_sha = hashlib.sha256(backup.read_bytes()).hexdigest()
        old_inode = (self.bin / "darius").stat().st_ino
        result = self.run_install(remote=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Make sure", result.stdout)
        self.assertNotEqual((self.bin / "darius").stat().st_ino, old_inode)
        self.assertEqual(subprocess.check_output([self.bin / "darius", "--version"], text=True).strip(), "darius 1.2.0")
        restore = self.bin / ".restore"
        shutil.copy2(backup, restore)
        os.replace(restore, self.bin / "darius")
        self.assertEqual(hashlib.sha256((self.bin / "darius").read_bytes()).hexdigest(), old_sha)
        self.assertEqual(subprocess.check_output([self.bin / "darius", "--version"], text=True).strip(), "darius 1.0.0")
        self.preserved()

    def test_http_missing_archive_checksum_and_corruption_preserve_old(self):
        self.serve()
        for failure in ("archive", "checksum", "corruption"):
            with self.subTest(failure=failure):
                self.package("darius 1.2.0")
                if failure == "archive":
                    (self.dist / self.asset).unlink()
                elif failure == "checksum":
                    (self.dist / self.asset.replace(".tar.gz", ".sha256")).unlink()
                else:
                    (self.dist / self.asset).write_bytes(b"corrupt")
                result = self.run_install(remote=True)
                self.assertNotEqual(result.returncode, 0)
                self.preserved()

if __name__ == "__main__":
    unittest.main()
