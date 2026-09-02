# Section 13 — Surface policy and configuration

## Scope and coverage

Reviewed all 4 assigned files under `crates/core/surface-config/` and
`crates/core/surface-policy/`, including policy/config models and tests.
Manifest, settings, shell, Wayland, presentation, and frontend callers were
searched. **4/4 assigned files inspected; no follow-up remains.**

## Process tree

```text
module manifest/profile/settings/props
  -> typed surface config and role policy validation
  -> sparse layer merge and semantic diff
  -> prepared shell/presentation configuration
  -> Wayland surface creation/update
  -> geometry/exclusive zone/blur/decorations commit
  -> generation acknowledgement, failure recovery, reload
```

## Performance findings

### S13-PERF-001 — Surface policy resolution repeats merge and serialization work

- **Source:** `crates/core/surface-config/src/lib.rs` and surface-policy
  resolution/validation paths.
- **Current behavior:** manifest, profile, user, and prop layers are merged and
  compared through owned configuration values when requests/reloads occur.
- **Why it matters:** frequent geometry or settings changes can cause repeated
  validation/serialization and unnecessary Wayland updates.
- **Recommended improvement:** compile a typed effective config per revision and
  expose a field-level semantic diff to presentation.
- **Measurement:** 1/10/100 fields and 1/10/1,000 surfaces with leaf versus
  full-layer changes; measure merges, allocations, serialized bytes, commits,
  and p95 update latency.
- **Confidence:** medium hypothesis. **Status:** new.

### S13-PERF-002 — Geometry changes can trigger broad downstream invalidation

- **Source:** surface config diff and shell/presentation consumers.
- **Current behavior:** placement, size, exclusive zone, blur, and decorations
  are compared as a surface-wide configuration.
- **Why it matters:** a field-independent change may recreate/recommit more
  state than necessary.
- **Recommended improvement:** retain semantic field groups and only schedule
  the affected Wayland protocol/update path.
- **Measurement:** isolated changes to size, margins, zone, blur, decorations,
  and role; measure commits, buffer work, damage, and frame gaps.
- **Confidence:** medium; **Status:** new measurement hypothesis.

## Dead code and redundancy

### S13-DEAD-001 — Role and surface configuration authorities overlap

- **Source:** surface-config models and surface-policy resolvers.
- **Current behavior:** role, promotability/ejection, geometry, and presentation
  policy are represented in adjacent structures and revalidated by callers.
- **Why it matters:** an accepted field can be ignored by one path or policy
  defaults can override explicit user intent.
- **Recommended improvement:** make one core typed policy result and keep shell/
  presentation adapters mechanical; remove wrappers only after call-graph audit.
- **Test:** generated field parity and cross-layer merge/diff fixtures.
- **Confidence:** medium redundancy; **Status:** older audit.

## Logic and core mechanics

### S13-LOGIC-001 — User settings must not bypass manifest policy guards

- **Source:** surface-config validation/merge and surface-policy role resolution.
- **Current behavior:** effective user/profile fields can be applied separately
  from author-declared guards such as promotability.
- **Why it matters:** settings could request an unsupported role/ejection or
  override a module's declared surface policy.
- **Recommended improvement:** validate each sparse layer against the effective
  typed policy, preserving provenance and rejecting unauthorized overrides.
- **Test:** author `promotable` true/false/omitted, user role changes, ejection,
  profile switch, and invalid enum values.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S13-LOGIC-002 — Invalid enum/policy values must fail closed, not select another role

- **Source:** surface config parsing and policy resolution enum conversion.
- **Current behavior:** malformed values can fall back to a different policy
  rather than producing a candidate diagnostic.
- **Why it matters:** a typo can silently change surface placement or privilege.
- **Recommended improvement:** reject invalid values in the canonical parser,
  retain the prior effective config, and report source/provenance.
- **Test:** every enum with invalid strings/numbers/null, profile/user precedence,
  and last-known-good behavior.
- **Confidence:** high. **Status:** older audit.

### S13-LOGIC-003 — Configuration diff must include every presentation-affecting field

- **Source:** surface-config semantic diff and presentation adapter callers.
- **Current behavior:** fields such as blur/window decorations/role can be
  omitted from change detection even though they affect protocol or paint.
- **Why it matters:** a valid setting change can be accepted but never committed
  to the compositor.
- **Recommended improvement:** derive exhaustive semantic diffs from the typed
  config and test each field's update path.
- **Test:** one-field-at-a-time changes for all config fields, including no-op
  equality and failed commit recovery.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S13-LOGIC-004 — Surface changes need one generation from settings to presentation

- **Source:** config/policy outputs and shell/Wayland presentation callers.
- **Current behavior:** settings role reload, explicit role requests, geometry,
  and presentation acknowledgements can commit through separate paths.
- **Why it matters:** compositor state and shell state can disagree after a
  partial failure or concurrent update.
- **Recommended improvement:** prepare one revisioned configuration and commit
  only after all required protocol changes are accepted; retain last-known-good.
- **Test:** concurrent settings/request changes, protocol failure, surface
  recreation, profile switch, and stale acknowledgement.
- **Confidence:** high architecture seam. **Status:** older audit.

## Existing backlog or audit overlap

The prior surface audit covers promotable guards, ignored settings, enum fallback,
incomplete presentation diffs, role reload transactions, inert fields,
localization/ejection identity, split geometry authority, and generations.
Those are overlap; no target-only behavior is reported as a current bug.

## Refuted suspicions

- No optimization from the rejected performance table is repeated. All
  performance items require field-count/surface-count measurements.

## Tests and benchmarks needed

- Sparse merge/provenance, policy authorization, invalid enum, exhaustive diff,
  generation, compositor failure, recreation, and rollback tests.
- Config resolution/update benchmarks with explicit surface/field counts,
  allocations, commits, bytes, p95 latency, and recovery results.

## File coverage

**Assigned:** 4/4 files in `crates/core/surface-config/` and
`crates/core/surface-policy/`. **Inspected:** 4/4. Manifest/settings/shell and
presentation callers were searched but belong to Sections 01, 02, 14, and 15.
**Files still needing review:** none.
