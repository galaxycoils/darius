"""Rules for authoritative prose and generated public text, not Rust symbols."""
import re

HIDDEN = r"(?<!/)\b(?:cron|approval-check|peer_send|MCP|subagent\w*|worktree|rollback|A2A)\b"
# Slash-prefixed tokens are URL path literals (e.g. /a2a/card in verified curl
# examples), not prose integration claims; prose mentions are still flagged.
QUALIFIED = r"unavailable|retired|removed|historical|not supported|not exposed|unverified"
BAD = [
    r"most powerful|guaranteed|across all normal and abnormal exits|upon any exit or signal",
    r"(?:under|less than|<)\s*2\s*(?:seconds|s\b)",
    r"(?:without|no|missing).*(?:API.key|provider|model.config).*(?:uses|runs with|fallback).*MockModel",
    r"Offline Mock Model \(Default\)",
    r"Safe Sandboxed Tools|zero home pollution",
]

def violations(text):
    errors = []
    for number, line in enumerate(text.splitlines(), 1):
        if any(re.search(rule, line, re.I) for rule in BAD):
            errors.append(f"{number}: unsupported guarantee or silent mock fallback: {line}")
        if re.search(HIDDEN, line, re.I) and not re.search(QUALIFIED, line, re.I):
            errors.append(f"{number}: unsupported integration must be qualified: {line}")
    return errors
