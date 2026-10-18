# Backend publication after successful callbacks

RT-07 removes the JSON round trip of the global `state` table. The Luau realm
owns mutable script state and table identity; Rust owns published service
snapshots. Failed commands retain their private mutations but publish no state,
result, or pending provider events. Successful callbacks validate their result
before taking a state snapshot. A suspended coroutine publishes only after it
finishes successfully; each completed task takes its own snapshot so a later
failure in the same wakeup cannot expose its partial mutations.

Keep existing successful-command and callback budget tests. Add regressions for
captured table identity, failure publication, suspended failure, and a successful
task followed by a failed task. This is a correctness change; make no speedup
claim from deleting the conversions.
