# Claude Code backlog runner — 2026-08-26

Consolidated from five same-day records about `scripts/claude-backlog-runner.sh`.

## 2026-08-26 — Claude Code backlog runner

`pending` · area: developer and authoring tools

Added `scripts/claude-backlog-runner.sh` and its structured-output schema. The
runner selects the next unchecked backlog item, invokes Claude Code through its
streaming JSON interface, requires a complete structured result, validates the
single backlog commit and planning record, and preserves unrelated dirty-tree
changes in a separate recovery commit. Each item starts a fresh Claude session
so context from one implementation cannot silently influence the next one.

## 2026-08-26 — Claude usage-limit retry

`pending` · area: developer and authoring tools

The Claude backlog runner now recognizes usage-limit and rate-limit failures in
Claude's structured stream errors or stderr, waits in bounded 60-second sleep
chunks, and retries the same backlog item indefinitely. The polling interval is
configurable with `CLAUDE_USAGE_RETRY_SECONDS` and defaults to five minutes.

## 2026-08-26 — Claude session-limit retry

`pending` · area: developer and authoring tools

Expanded the Claude runner's retry matcher to include Claude Code's actual
`You've hit your session limit` response, including the broader `hit your …
limit` wording. Session-limit failures now wait for the configured interval and
retry instead of terminating the backlog loop.

## 2026-08-26 — Claude unattended permissions

`pending` · area: developer and authoring tools

The Claude backlog runner now always includes
`--dangerously-skip-permissions` in every invocation. The previous environment
override could disable the bypass and leave an unattended run waiting for an
interactive permission prompt.

## 2026-08-26 — Claude schema compatibility

`pending` · area: developer and authoring tools

Removed the Draft 2020-12 `$schema` declaration from the Claude runner's
structured-output schema. Claude Code rejected that meta-schema before starting
the first turn; the schema remains valid JSON Schema while allowing Claude's
CLI validator to select its supported dialect.
