"""Validate tagged-source provenance before generating release evidence."""
import re
import subprocess


def provenance(repo, tag, baseline, supplied):
    def git(*args):
        result = subprocess.run(["git", *args], cwd=repo, text=True, capture_output=True)
        if result.returncode:
            raise ValueError(f"Invalid release provenance: git {args[0]} failed")
        return result.stdout.strip()

    if not re.fullmatch(r"[0-9a-f]{40}", baseline):
        raise ValueError("Baseline must be a full 40-character commit SHA")
    head = git("rev-parse", "HEAD")
    if git("rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}") != head:
        raise ValueError("Release tag does not identify HEAD")
    if git("diff", "HEAD", "--name-only"):
        raise ValueError("Tracked source differs from tagged HEAD")
    git("merge-base", "--is-ancestor", baseline, head)
    work = git("rev-list", "--reverse", f"{baseline}..{head}").splitlines()
    if supplied and supplied != work:
        raise ValueError("WU SHAs do not match complete ordered release history")
    return head, work
