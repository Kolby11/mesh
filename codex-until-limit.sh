#!/usr/bin/env bash

set -uo pipefail

# Repeatedly run Codex until account usage is exhausted. When the current
# session is estimated to have 20% or less of its context window remaining, ask
# Codex to write a handoff and continue in a fresh conversation.
#
# Usage:
#   ./codex-until-limit.sh "Implement the remaining project backlog"
#
# Configuration:
#   CODEX_CONTEXT_WINDOW=128000     Fallback model context window in tokens
#   CODEX_MIN_CONTEXT_PERCENT=20    Start a fresh conversation at this remainder
#   CODEX_WORKDIR="$PWD"            Repository Codex should work in
#   CODEX_HANDOFF_FILE=.codex-continuation.md
#   CODEX_LOOP_LOG=codex-loop.log

usage() {
    cat <<'EOF'
Usage: codex-until-limit.sh [TASK]

Continuously runs Codex on TASK. At approximately 20% context remaining it
writes a handoff and starts a fresh conversation. It stops when usage is
exhausted, after three consecutive failures, when interrupted, or when the
file .codex-loop.stop exists in the work directory.

Set CODEX_CONTEXT_WINDOW to the context-window value shown by Codex /status.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
fi

for command in codex jq; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "Required command not found: $command" >&2
        exit 127
    fi
done

WORKDIR="${CODEX_WORKDIR:-$PWD}"
if [[ -n "${CODEX_CONTEXT_WINDOW:-}" ]]; then
    CONTEXT_WINDOW_EXPLICIT=1
else
    CONTEXT_WINDOW_EXPLICIT=0
fi
CONTEXT_WINDOW="${CODEX_CONTEXT_WINDOW:-128000}"
MIN_CONTEXT_PERCENT="${CODEX_MIN_CONTEXT_PERCENT:-20}"
HANDOFF_FILE="${CODEX_HANDOFF_FILE:-$WORKDIR/.codex-continuation.md}"
LOG_FILE="${CODEX_LOOP_LOG:-$WORKDIR/codex-loop.log}"
STOP_FILE="$WORKDIR/.codex-loop.stop"

TASK="${*:-Work through the documented project backlog. Choose the highest-priority unfinished item, implement a coherent increment, run the relevant tests, and continue until no usage remains. Avoid unrelated changes.}"

if [[ ! "$CONTEXT_WINDOW" =~ ^[1-9][0-9]*$ ]]; then
    echo "CODEX_CONTEXT_WINDOW must be a positive integer." >&2
    exit 2
fi

if [[ ! "$MIN_CONTEXT_PERCENT" =~ ^[0-9]+$ ]] ||
   (( MIN_CONTEXT_PERCENT < 1 || MIN_CONTEXT_PERCENT > 99 )); then
    echo "CODEX_MIN_CONTEXT_PERCENT must be an integer from 1 to 99." >&2
    exit 2
fi

if [[ ! -d "$WORKDIR" ]]; then
    echo "Work directory does not exist: $WORKDIR" >&2
    exit 2
fi

FALLBACK_ROLLOVER_AT=$((CONTEXT_WINDOW * (100 - MIN_CONTEXT_PERCENT) / 100))
TEMP_DIR="$(mktemp -d)"
TURN_JSON="$TEMP_DIR/turn.jsonl"
TURN_ERROR="$TEMP_DIR/turn.stderr"

cleanup() {
    rm -rf "$TEMP_DIR"
}

trap cleanup EXIT
trap 'exit 130' INT TERM

run_codex() {
    : >"$TURN_JSON"
    : >"$TURN_ERROR"

    "$@" >"$TURN_JSON" 2>"$TURN_ERROR"
    local status=$?

    tee -a "$LOG_FILE" <"$TURN_ERROR" >&2
    tee -a "$LOG_FILE" <"$TURN_JSON"
    return "$status"
}

usage_error_pattern='(^|[^[:alpha:]])(usage[[:space:]_-]+limit|quota[[:space:]_-]+(exceeded|exhausted)|insufficient[[:space:]_-]+quota|credits?[[:space:]_-]+(exhausted|depleted)|out[[:space:]]+of[[:space:]]+credits)([^[:alpha:]]|$)'

codex_sessions_dir() {
    if [[ -n "${CODEX_HOME:-}" ]]; then
        printf '%s/sessions\n' "$CODEX_HOME"
        return 0
    fi

    command -v getent >/dev/null 2>&1 || return 1
    local user_home
    user_home=$(getent passwd "$(id -u)" | cut -d: -f6)
    [[ -n "$user_home" ]] || return 1
    printf '%s/.codex/sessions\n' "$user_home"
}

rollout_file_for_thread() {
    local thread=$1 sessions_dir
    [[ -n "$thread" ]] || return 1
    sessions_dir=$(codex_sessions_dir) || return 1
    [[ -d "$sessions_dir" ]] || return 1
    find "$sessions_dir" -type f -name "*-$thread.jsonl" -print -quit 2>/dev/null
}

usage_exhausted() {
    # Never scan the complete JSONL stream for keywords: command output is
    # embedded in item.completed events and may legitimately contain words
    # such as "quotas".
    if jq -e -s '
        def error_text:
            [
                .message?,
                .code?,
                .error?.message?,
                .error?.code?,
                .item?.message?,
                .item?.code?,
                .item?.error?.message?,
                .item?.error?.code?
            ]
            | map(select(type == "string"))
            | join(" ")
            | ascii_downcase;

        any(.[];
            ((.type == "error")
              or (.type == "item.completed" and .item.type == "error")
              or (.type == "turn.failed"))
            and (error_text | test("usage[ _-]+limit|quota[ _-]+(exceeded|exhausted)|insufficient[ _-]+quota|credits?[ _-]+(exhausted|depleted)|out of credits"))
        )
    ' "$TURN_JSON" >/dev/null 2>&1; then
        return 0
    fi

    # Some CLI/API failures are emitted only on stderr. This is intentionally
    # a narrow phrase match and excludes generic/transient "rate limit" text.
    if grep -Eiq "$usage_error_pattern" "$TURN_ERROR"; then
        return 0
    fi

    local thread=${1:-} rollout_file
    rollout_file=$(rollout_file_for_thread "$thread" || true)
    [[ -n "$rollout_file" ]] || return 1

    # The persisted rollout carries the structured account-window result that
    # is not included in `codex exec --json` stdout.
    jq -e -s '
        [
            .[]
            | select(.type == "event_msg" and .payload.type == "token_count")
        ]
        | if length == 0 then false
          else ((.[-1].payload.rate_limits.rate_limit_reached_type // "") | tostring | length > 0)
          end
    ' "$rollout_file" >/dev/null 2>&1
}

context_usage() {
    local thread=$1 rollout_file
    rollout_file=$(rollout_file_for_thread "$thread" || true)
    [[ -n "$rollout_file" ]] || return 1

    jq -r -s '
        [
            .[]
            | select(.type == "event_msg" and .payload.type == "token_count" and .payload.info != null)
            | .payload.info
        ]
        | if length == 0 then empty
          else .[-1]
          | [(.last_token_usage.total_tokens // 0), (.model_context_window // 0)]
          | @tsv
          end
    ' "$rollout_file"
}

thread_id=""
failures=0
next_prompt="$TASK"

echo "Starting Codex loop in $WORKDIR" | tee -a "$LOG_FILE"
echo "Fallback context rollover threshold: $FALLBACK_ROLLOVER_AT / $CONTEXT_WINDOW tokens used" | tee -a "$LOG_FILE"
echo "Create $STOP_FILE to stop after the current turn." | tee -a "$LOG_FILE"

while [[ ! -e "$STOP_FILE" ]]; do
    if [[ -z "$thread_id" ]]; then
        command=(
            codex exec
            --json
            --sandbox workspace-write
            -C "$WORKDIR"
            "$next_prompt"
        )
    else
        command=(
            codex exec resume
            --json
            "$thread_id"
            "Continue implementing the objective. Inspect the current repository state, choose the next useful increment, implement it, and run relevant tests. Do not stop merely because a previous increment completed."
        )
    fi

    run_codex "${command[@]}"
    status=$?

    if [[ -z "$thread_id" ]]; then
        thread_id="$(jq -r 'select(.type == "thread.started") | .thread_id // empty' "$TURN_JSON" | head -n 1)"
    fi

    if usage_exhausted "$thread_id"; then
        echo "Codex usage appears exhausted; stopping." | tee -a "$LOG_FILE"
        exit 0
    fi

    if (( status != 0 )); then
        ((failures += 1))
        echo "Codex exited with status $status ($failures/3)." | tee -a "$LOG_FILE"
        if (( failures >= 3 )); then
            exit "$status"
        fi
        sleep 60
        continue
    fi

    failures=0

    if [[ -z "$thread_id" ]]; then
        echo "Could not find the Codex thread ID in JSON output." >&2
        exit 1
    fi

    context_usage_value="$(context_usage "$thread_id" || true)"
    if [[ -z "$context_usage_value" ]]; then
        echo "Current context usage was unavailable; starting a fresh conversation conservatively." \
            | tee -a "$LOG_FILE"
        thread_id=""
        next_prompt="Inspect the current repository state and continue implementing the objective: $TASK"
        continue
    fi

    read -r used_tokens reported_context_window <<<"$context_usage_value"
    effective_context_window="$CONTEXT_WINDOW"
    if (( !CONTEXT_WINDOW_EXPLICIT && reported_context_window > 0 )); then
        effective_context_window="$reported_context_window"
    fi

    if (( effective_context_window <= 0 )); then
        echo "Invalid context window; starting a fresh conversation conservatively." \
            | tee -a "$LOG_FILE"
        thread_id=""
        next_prompt="Inspect the current repository state and continue implementing the objective: $TASK"
        continue
    fi

    echo "Estimated context usage: $used_tokens / $effective_context_window tokens" \
        | tee -a "$LOG_FILE"

    rollover_at=$((effective_context_window * (100 - MIN_CONTEXT_PERCENT) / 100))
    if (( used_tokens >= rollover_at )); then
        handoff_prompt="Context rollover is imminent. Write or replace $HANDOFF_FILE with a concise continuation handoff. Include the objective, constraints, completed work, repository and git state, files changed, architectural decisions, tests and results, unresolved problems, and exact next steps. Do not perform further implementation in this turn."

        run_codex codex exec resume --json "$thread_id" "$handoff_prompt"
        status=$?

        if usage_exhausted "$thread_id"; then
            echo "Codex usage was exhausted while writing the handoff." | tee -a "$LOG_FILE"
            exit 0
        fi

        if (( status != 0 )); then
            echo "Unable to create the context handoff; stopping safely." >&2
            exit "$status"
        fi

        thread_id=""
        next_prompt="Read $HANDOFF_FILE and inspect the current repository state. Continue implementing the original objective: $TASK. Treat the handoff as a summary rather than unquestioned truth, verify the current state, and continue autonomously."
        echo "Handoff written; the next turn will use a fresh conversation." | tee -a "$LOG_FILE"
    fi
done

echo "Stop file detected; exiting." | tee -a "$LOG_FILE"
