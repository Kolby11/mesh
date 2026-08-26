## 2026-08-26 — Claude usage-limit retry

`pending` · area: developer and authoring tools

The Claude backlog runner now recognizes usage-limit and rate-limit failures in
Claude's structured stream errors or stderr, waits in bounded 60-second sleep
chunks, and retries the same backlog item indefinitely. The polling interval is
configurable with `CLAUDE_USAGE_RETRY_SECONDS` and defaults to five minutes.
