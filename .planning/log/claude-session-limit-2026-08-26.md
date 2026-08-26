## 2026-08-26 — Claude session-limit retry

`pending` · area: developer and authoring tools

Expanded the Claude runner's retry matcher to include Claude Code's actual
`You've hit your session limit` response, including the broader `hit your …
limit` wording. Session-limit failures now wait for the configured interval and
retry instead of terminating the backlog loop.
