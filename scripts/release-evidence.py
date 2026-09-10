#!/usr/bin/env python3
"""Generate release-evidence.json with baseline/WU commit SHAs and artifact digests.

Frozen-plan gate: the release job must produce evidence that the tagged SHA
matches the workspace version, every archive checksum is recorded, and the
baseline + every completed WU commit SHA are pinned for rollback auditing.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from release_provenance import provenance

REQUIRED_ASSETS = (
    "darius-linux-x86_64",
    "darius-macos-aarch64",
    "darius-macos-x86_64",
    "darius-windows-x86_64",
)

def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def cargo_package_version(repo: Path) -> str:
    import json as _json
    meta = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=repo,
        capture_output=True,
        text=True,
        check=True,
    )
    data = _json.loads(meta.stdout)
    for pkg in data["packages"]:
        if pkg["name"] == "darius-cli":
            return pkg["version"]
    raise SystemExit("darius-cli package not found in workspace metadata")


def git_head(repo: Path) -> str:
    return subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repo,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", type=Path, required=True)
    ap.add_argument("--dist", type=Path, required=True)
    ap.add_argument("--tag", required=True)
    ap.add_argument("--baseline", required=True)
    ap.add_argument("--wu-shas", nargs="*", default=[])
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()

    repo = args.repo.resolve()
    dist = args.dist.resolve()
    version = cargo_package_version(repo)
    expected_tag = f"v{version}"
    if args.tag != expected_tag:
        print(f"Tag mismatch: {args.tag} != {expected_tag}", file=sys.stderr)
        return 1

    try:
        commit_sha, wu_shas = provenance(repo, args.tag, args.baseline, args.wu_shas)
    except ValueError as error:
        print(str(error), file=sys.stderr)
        return 1

    artifacts = {}
    for stem in REQUIRED_ASSETS:
        tar = dist / f"{stem}.tar.gz"
        zip_file = dist / f"{stem}.zip"
        archive = zip_file if zip_file.exists() else tar
        checksum = dist / f"{stem}.sha256"
        if not archive.exists() or not checksum.exists():
            print(f"Missing artifact or checksum: {stem}", file=sys.stderr)
            return 1
        expected = checksum.read_text().split()[0]
        actual = sha256(archive)
        if expected != actual:
            print(f"Checksum mismatch for {stem}", file=sys.stderr)
            return 1
        artifacts[archive.name] = actual
        artifacts[f"{stem}.sha256"] = sha256(checksum)
    if not (dist / "install.sh").exists():
        print("Missing install.sh in dist", file=sys.stderr)
        return 1
    artifacts["install.sh"] = sha256(dist / "install.sh")

    evidence = {
        "tag": args.tag,
        "version": version,
        "commit_sha": commit_sha,
        "baseline_sha": args.baseline,
        "wu_shas": wu_shas,
        "timestamp": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "artifacts": artifacts,
    }
    out = args.out.resolve()
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(evidence, indent=2) + "\n")
    print(f"Wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
