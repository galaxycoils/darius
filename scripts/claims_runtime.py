"""Exercise fresh CLI help, diagnostic and installer output in an isolated home."""
import os
import re
import subprocess
import tempfile
from claims_policy import violations

EXPECTED = {(): {"tui", "run", "config", "memory"}, ("config",): {"show", "init", "preset"}, ("memory",): {"search", "pack", "import", "export", "stats"}}

def audit(root, binary):
    errors = []
    with tempfile.TemporaryDirectory(prefix="darius-claims-") as home:
        env = {k: v for k, v in os.environ.items() if not k.endswith("API_KEY")}
        env.update(DARIUS_HOME=home, HOME=home)
        pending: list[tuple[str, ...]] = [()]
        while pending:
            args = pending.pop()
            result = subprocess.run([binary, *args, "--help"], env=env, capture_output=True, text=True, check=True)
            text = result.stdout + result.stderr
            errors.extend(f"help {args}: {e}" for e in violations(text))
            section = text.split("Commands:\n", 1)[-1].split("\n\n", 1)[0] if "Commands:\n" in text else ""
            names = set(re.findall(r"^  ([a-z][a-z-]*)\s", section, re.M))
            if names != EXPECTED.get(args, set()):
                errors.append(f"unexpected command surface {args}: {names}")
            pending.extend((*args, name) for name in names)
        result = subprocess.run([binary, "config", "show"], env=env, capture_output=True, text=True, check=True)
        text = result.stdout + result.stderr
        errors.extend(violations(text))
        if "Runtime state: setup" not in text or "Provider URL: not configured" not in text:
            errors.append("clean-home diagnostics claimed configured provider state")
        database = __import__('pathlib').Path(home) / 'profiles/default/memory.db'
        if ("Memory: open" in text) != database.is_file():
            errors.append("diagnostics memory state disagrees with local storage")
        result = subprocess.run(["bash", "install.sh", "--help"], cwd=root, env=env, capture_output=True, text=True, check=True)
        errors.extend(violations(result.stdout + result.stderr))
    return errors
