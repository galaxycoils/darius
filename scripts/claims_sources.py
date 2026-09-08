"""Bound the audit to authoritative content and machine-visible literals."""
from pathlib import Path
import re
from claims_policy import violations

DOCS = ("README.md", "CHANGELOG.md", "docs/CAPABILITIES.md", "docs/TROUBLESHOOTING.md")

def audit(root):
    errors = []
    for name in DOCS:
        text = (root / name).read_text()
        errors.extend(f"{name}:{error}" for error in violations(text))
    changelog = (root / "CHANGELOG.md").read_text()
    for marker in ("[1.2.0] - Unreleased", "[1.1.2]", "Corrected"):
        if marker not in changelog:
            errors.append(f"CHANGELOG.md missing correction marker {marker}")
    for path in [root / "Cargo.toml", *root.glob("crates/*/Cargo.toml"), *root.glob("tests/*/Cargo.toml")]:
        for description in re.findall(r'^description\s*=\s*"(.*)"', path.read_text(), re.M):
            errors.extend(f"{path}:{error}" for error in violations(description))
    for name in ("install.sh", ".github/workflows/release.yml", "scripts/release-evidence.py"):
        text = (root / name).read_text()
        visible = "\n".join(line for line in text.splitlines() if re.search(r"echo |printf |body:|release.notes|description", line))
        errors.extend(f"{name}:{error}" for error in violations(visible))
    doc = (root / "docs/CAPABILITIES.md").read_text()
    for row in doc.splitlines():
        if "**Verified**" not in row:
            continue
        links = re.findall(r'\[`(\w+)`\]\(([^)#]+)(?:#[^)]*)?\)', row)
        if not links:
            errors.append(f"Verified row needs named executable proof: {row}")
        for function, relative in links:
            path = root / "docs" / relative
            if not path.is_file() or not re.search(r"#\[(?:tokio::)?test\]\s*(?:async )?fn " + function + r"\(", path.read_text()):
                errors.append(f"missing proof function: {relative}:{function}")
    return errors
