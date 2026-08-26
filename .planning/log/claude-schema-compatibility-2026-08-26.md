## 2026-08-26 — Claude schema compatibility

`pending` · area: developer and authoring tools

Removed the Draft 2020-12 `$schema` declaration from the Claude runner's
structured-output schema. Claude Code rejected that meta-schema before starting
the first turn; the schema remains valid JSON Schema while allowing Claude's
CLI validator to select its supported dialect.
