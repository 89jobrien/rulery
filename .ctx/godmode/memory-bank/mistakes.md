---
version: 1
last_updated: 2026-09-21
next_review: 2026-09-28
---

# Recurring Mistakes Ledger

## Process Errors

### POSIX shell syntax used in Nushell command context

- **Occurrences**: 3
- **Dates**: 2026-09-21
- **Affected**: Repository setup and report-directory creation commands
- **Prevention**: Use separate tool calls or Nushell `;`/`and`; use Nushell-native `mkdir` without `-p` after verifying each parent directory.
- **Notes**: `git remote -v && ...` and `git diff && git diff --cached` failed because `&&` is unsupported, and `mkdir -p` failed because Nushell's `mkdir` has no `-p` flag.
