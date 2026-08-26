## 2026-08-26 — Claude unattended permissions

`pending` · area: developer and authoring tools

The Claude backlog runner now always includes
`--dangerously-skip-permissions` in every invocation. The previous environment
override could disable the bypass and leave an unattended run waiting for an
interactive permission prompt.
