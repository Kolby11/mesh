#!/usr/bin/env bash

# Run one Codex turn per backlog item. Every successful turn must leave exactly
# one new commit on main, remove its backlog item, and add the required log
# record before the next item is attempted.

set -Eeuo pipefail
IFS=$'\n\t'

usage() {
    cat <<'EOF'
Usage: scripts/codex-backlog-runner.sh [options]

Run Codex against the next unchecked item in docs/BACKLOG.md. The runner only
operates on a clean main branch and requires exactly one new commit per item.

Options:
  --dry-run                 Show the next item and configuration without running Codex
  --max-features N          Stop after N successful items (default: unlimited)
  --once                    Alias for --max-features 1
  -h, --help                Show this help

Environment:
  CODEX_MODEL               Optional model override; otherwise Codex config applies
  CODEX_CONTEXT_WINDOW_TOKENS
                            Context size used for rollover calculations
                            (default: 1050000 for GPT-5.6)
  CODEX_CONTEXT_LEFT_PERCENT
                            Start a fresh session at or below this percentage
                            of context remaining (default: 30)
  CODEX_ALLOW_ALL          Set to 0 to use workspace-write instead
                            (default: 1; unattended mode bypasses approvals)
  CODEX_AUTO_APPROVE        Set to 0 to omit --approve-for-me (default: 1)
  CODEX_BIN                 Codex executable (default: codex)
EOF
}

die() {
    printf 'codex-backlog-runner: %s\n' "$*" >&2
    exit 1
}

MAX_FEATURES=0
DRY_RUN=0

while (($# > 0)); do
    case "$1" in
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        --max-features)
            (($# >= 2)) || die "--max-features requires a positive integer"
            MAX_FEATURES=$2
            [[ "$MAX_FEATURES" =~ ^[1-9][0-9]*$ ]] || die "--max-features must be a positive integer"
            shift 2
            ;;
        --once)
            MAX_FEATURES=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            die "unknown option: $1"
            ;;
    esac
done

CODEX_BIN=${CODEX_BIN:-codex}
CODEX_CONTEXT_WINDOW_TOKENS=${CODEX_CONTEXT_WINDOW_TOKENS:-1050000}
CODEX_CONTEXT_LEFT_PERCENT=${CODEX_CONTEXT_LEFT_PERCENT:-30}
CODEX_ALLOW_ALL=${CODEX_ALLOW_ALL:-1}
CODEX_AUTO_APPROVE=${CODEX_AUTO_APPROVE:-1}
CODEX_MODEL=${CODEX_MODEL:-}

[[ "$CODEX_CONTEXT_WINDOW_TOKENS" =~ ^[1-9][0-9]*$ ]] || die "CODEX_CONTEXT_WINDOW_TOKENS must be a positive integer"
[[ "$CODEX_CONTEXT_LEFT_PERCENT" =~ ^([1-9][0-9]?|100)$ ]] || die "CODEX_CONTEXT_LEFT_PERCENT must be between 1 and 100"
[[ "$CODEX_ALLOW_ALL" == 0 || "$CODEX_ALLOW_ALL" == 1 ]] || die "CODEX_ALLOW_ALL must be 0 or 1"
[[ "$CODEX_AUTO_APPROVE" == 0 || "$CODEX_AUTO_APPROVE" == 1 ]] || die "CODEX_AUTO_APPROVE must be 0 or 1"

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(git -C "$SCRIPT_DIR/.." rev-parse --show-toplevel 2>/dev/null) || die "not inside a Git repository"
BACKLOG="$ROOT/docs/BACKLOG.md"
SCHEMA="$SCRIPT_DIR/codex-backlog-runner.schema.json"

[[ -f "$BACKLOG" ]] || die "missing $BACKLOG"
[[ -f "$SCHEMA" ]] || die "missing $SCHEMA"
command -v "$CODEX_BIN" >/dev/null 2>&1 || die "Codex executable not found: $CODEX_BIN"
command -v jq >/dev/null 2>&1 || die "jq is required to parse Codex JSONL output"

next_backlog_line() {
    awk '/^- \[ \] / { print NR; exit }' "$BACKLOG"
}

backlog_item_at() {
    local line=$1
    sed -n "${line}p" "$BACKLOG"
}

assert_main_and_clean() {
    local branch
    branch=$(git -C "$ROOT" branch --show-current)
    [[ "$branch" == main ]] || die "refusing to run on '$branch'; checkout main first"

    if [[ -n "$(git -C "$ROOT" status --porcelain)" ]]; then
        git -C "$ROOT" status --short >&2
        die "working tree is not clean; commit or set aside existing changes first"
    fi
}

print_configuration() {
    local rollover_used=$((CODEX_CONTEXT_WINDOW_TOKENS * (100 - CODEX_CONTEXT_LEFT_PERCENT) / 100))
    printf 'repository: %s\n' "$ROOT"
    printf 'branch: %s\n' "$(git -C "$ROOT" branch --show-current)"
    printf 'context window: %s tokens\n' "$CODEX_CONTEXT_WINDOW_TOKENS"
    printf 'fresh-session threshold: %s tokens used (%s%% remaining)\n' "$rollover_used" "$CODEX_CONTEXT_LEFT_PERCENT"
    printf 'allow all permissions: %s\n' "$CODEX_ALLOW_ALL"
    printf 'auto approval: %s\n' "$CODEX_AUTO_APPROVE"
}

if [[ -z "$(next_backlog_line)" ]]; then
    printf 'No unchecked backlog items remain.\n'
    exit 0
fi

if ((DRY_RUN)); then
    print_configuration
    line=$(next_backlog_line)
    printf 'next item: %s\n' "$(backlog_item_at "$line")"
    exit 0
fi

assert_main_and_clean
cd -- "$ROOT"

TMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/mesh-codex-runner.XXXXXX")
trap 'rm -rf -- "$TMP_DIR"' EXIT

session_id=''
successful_features=0

build_prompt() {
    local line=$1
    local item
    item=$(backlog_item_at "$line")

    cat <<EOF
You are one worker in an unattended MESH backlog implementation loop.

Implement exactly the unchecked backlog item currently beginning at line $line
of docs/BACKLOG.md:

$item

Before editing:
- Read AGENTS.md, docs/architecture/overview.md, docs/spec/README.md,
  docs/BACKLOG.md, .planning/STATUS.md, .planning/README.md, and the relevant
  current log/audit files.
- Re-read docs/BACKLOG.md immediately before deciding the item; line numbers
  may have shifted since this prompt was prepared.
- Check the existing code and tests, and inspect the relevant history when it
  prevents repeating an earlier failed approach.

Implementation rules:
- Work directly on the existing main branch. Do not create or switch branches,
  worktrees, or commits for unrelated work.
- Implement only this one backlog item. Do not start another item, refactor
  unrelated code, or edit frozen .planning/archive/ files.
- Preserve normal repository terminology and architecture boundaries.
- Run focused tests first, then the broadest relevant validation that is
  practical. Record real failures rather than claiming success.
- When the item is genuinely complete, delete its unchecked item from
  docs/BACKLOG.md, append its dated record to the current .planning/log/YYYY-MM.md
  (or the performance log when this is performance work), and update
  .planning/STATUS.md only if the in-flight work changed.
- Create exactly one commit containing the implementation and the required
  tracking updates. Do not amend, reset, rebase, or create a second commit.
- Do not push to any remote.
- If the item is blocked or cannot be completed safely, do not fake completion,
  do not remove it from the backlog, and do not commit. Leave useful diagnostics
  in the final response and stop.

Final response:
Return only JSON matching the supplied schema. Set status to complete only if
the single commit was created and validation passed. Include the commit hash,
the tests actually run, and a concise summary.
EOF
}

run_turn() {
    local prompt=$1
    local event_file=$2
    local stderr_file=$3
    local -a command

    if [[ -n "$session_id" ]]; then
        command=("$CODEX_BIN" exec resume "$session_id" --json --output-schema "$SCHEMA")
        if ((CODEX_ALLOW_ALL)); then
            command+=(--dangerously-bypass-approvals-and-sandbox)
        fi
    else
        command=("$CODEX_BIN" exec --json --output-schema "$SCHEMA")
        if [[ -n "$CODEX_MODEL" ]]; then
            command+=(--model "$CODEX_MODEL")
        fi
        if ((CODEX_ALLOW_ALL)); then
            command+=(--dangerously-bypass-approvals-and-sandbox)
        elif ((CODEX_AUTO_APPROVE)); then
            command+=(--approve-for-me)
        else
            command+=(--sandbox workspace-write)
        fi
    fi

    set +e
    "${command[@]}" "$prompt" >"$event_file" 2>"$stderr_file"
    local result=$?
    set -e
    cat "$stderr_file" >&2
    cat "$event_file"
    return "$result"
}

read_thread_id() {
    jq -r -s '
        map(select(.type == "thread.started") | .thread_id)
        | if length == 0 then "" else .[0] end
    ' "$1"
}

read_usage() {
    jq -r '
        select(.type == "turn.completed" and .usage != null)
        | [
            (.usage.input_tokens // 0),
            (.usage.output_tokens // 0),
            (.usage.reasoning_output_tokens // 0)
          ]
        | @tsv
    ' "$1" | tail -n 1
}

read_final_response() {
    jq -r -s '
        map(select(.type == "item.completed" and .item.type == "agent_message") | .item.text)
        | if length == 0 then "" else .[-1] end
    ' "$1"
}

verify_feature_commit() {
    local before=$1
    local marker=$2
    local after commit_count parent_count

    if [[ -n "$(git -C "$ROOT" status --porcelain)" ]]; then
        git -C "$ROOT" status --short >&2
        die "Codex left uncommitted changes after its turn"
    fi

    after=$(git -C "$ROOT" rev-parse HEAD)
    git -C "$ROOT" merge-base --is-ancestor "$before" "$after" || die "history was rewritten during the turn"
    commit_count=$(git -C "$ROOT" rev-list --count "$before..$after")
    [[ "$commit_count" == 1 ]] || die "expected exactly one new commit, found $commit_count"

    parent_count=$(git -C "$ROOT" rev-list --parents -n 1 "$after" | awk '{ print NF - 1 }')
    [[ "$parent_count" == 1 ]] || die "the feature commit must not be a merge commit"

    if git -C "$ROOT" diff --quiet "$before" "$after" -- docs/BACKLOG.md; then
        die "feature commit did not update docs/BACKLOG.md"
    fi
    if grep -Fq -- "$marker" "$BACKLOG"; then
        die "the completed backlog item is still present"
    fi
    if ! git -C "$ROOT" diff --name-only "$before" "$after" -- .planning/log | grep -q .; then
        die "feature commit did not add the required planning log record"
    fi

    printf '%s\n' "$after"
}

while :; do
    if ((MAX_FEATURES > 0 && successful_features >= MAX_FEATURES)); then
        printf 'Reached --max-features=%s.\n' "$MAX_FEATURES"
        exit 0
    fi

    line=$(next_backlog_line)
    if [[ -z "$line" ]]; then
        printf 'No unchecked backlog items remain.\n'
        exit 0
    fi

    marker=$(backlog_item_at "$line")
    before=$(git -C "$ROOT" rev-parse HEAD)
    event_file="$TMP_DIR/turn-$((successful_features + 1)).jsonl"
    stderr_file="$TMP_DIR/turn-$((successful_features + 1)).stderr"
    prompt=$(build_prompt "$line")

    printf '\n=== feature %s: %s ===\n' "$((successful_features + 1))" "$marker"
    if ! run_turn "$prompt" "$event_file" "$stderr_file"; then
        die "Codex turn failed; inspect the repository and the captured session before retrying"
    fi

    new_session_id=$(read_thread_id "$event_file")
    if [[ -n "$new_session_id" ]]; then
        session_id=$new_session_id
    fi

    final_response=$(read_final_response "$event_file")
    [[ -n "$final_response" ]] || die "Codex did not return its required structured result"
    jq -e 'type == "object" and .status == "complete"' >/dev/null <<<"$final_response" \
        || die "Codex did not report a complete feature result"
    printf 'Codex result: %s\n' "$final_response"

    commit=$(verify_feature_commit "$before" "$marker")
    successful_features=$((successful_features + 1))
    printf 'Committed feature %s as %s\n' "$successful_features" "$commit"

    usage=$(read_usage "$event_file")
    if [[ -z "$usage" ]]; then
        printf 'No usage event was reported; starting a fresh session conservatively.\n' >&2
        session_id=''
        continue
    fi
    read -r input_tokens output_tokens reasoning_tokens <<<"$usage"
    used_tokens=$((input_tokens + output_tokens + reasoning_tokens))
    remaining_percent=$(( (CODEX_CONTEXT_WINDOW_TOKENS - used_tokens) * 100 / CODEX_CONTEXT_WINDOW_TOKENS ))
    printf 'Context estimate: %s tokens used, %s%% remaining.\n' "$used_tokens" "$remaining_percent"
    if ((remaining_percent <= CODEX_CONTEXT_LEFT_PERCENT)); then
        printf 'Starting the next feature in a fresh Codex session.\n'
        session_id=''
    fi
done
