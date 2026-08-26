## 2026-08-26 — Claude Code backlog runner

`pending` · area: developer and authoring tools

Added `scripts/claude-backlog-runner.sh` and its structured-output schema. The
runner selects the next unchecked backlog item, invokes Claude Code through its
streaming JSON interface, requires a complete structured result, validates the
single backlog commit and planning record, and preserves unrelated dirty-tree
changes in a separate recovery commit. Each item starts a fresh Claude session
so context from one implementation cannot silently influence the next one.
