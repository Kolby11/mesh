#!/usr/bin/env bash

set -uo pipefail

# Repeatedly run Codex until account usage is exhausted. When the most recent
# turn is estimated to have 20% or less of its context window remaining, ask
# Codex to write a handoff and continue in a fresh conversation.
#
# Usage:
#   ./codex-until-limit.sh "Implement the remaining project backlog"
#
# Configuration:
#   CODEX_CONTEXT_WINDOW=128000     Model context window in tokens
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

ROLLOVER_AT=$((CONTEXT_WINDOW * (100 - MIN_CONTEXT_PERCENT) / 100))
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

usage_exhausted() {
    grep -Eqi \
        'usage limit|rate limit|quota|insufficient_quota|credits.{0,30}(exhaust|deplet)' \
        "$TURN_JSON" "$TURN_ERROR"
}

context_tokens() {
    jq -s '
        [
            .[]
            | select(.type == "turn.completed")
            | .usage
            | ((.input_tokens // 0)
              + (.output_tokens // 0)
              + (.reasoning_output_tokens // 0))
        ]
        | max // 0
    ' "$TURN_JSON" 2>/dev/null || echo 0
}

thread_id=""
failures=0
next_prompt="$TASK"

echo "Starting Codex loop in $WORKDIR" | tee -a "$LOG_FILE"
echo "Context rollover threshold: $ROLLOVER_AT / $CONTEXT_WINDOW tokens used" | tee -a "$LOG_FILE"
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

    if usage_exhausted; then
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
        thread_id="$(jq -r 'select(.type == "thread.started") | .thread_id // empty' "$TURN_JSON" | head -n 1)"
        if [[ -z "$thread_id" ]]; then
            echo "Could not find the Codex thread ID in JSON output." >&2
            exit 1
        fi
    fi

    used_tokens="$(context_tokens)"
    echo "Estimated context usage: $used_tokens / $CONTEXT_WINDOW tokens" | tee -a "$LOG_FILE"

    if (( used_tokens >= ROLLOVER_AT )); then
        handoff_prompt="Context rollover is imminent. Write or replace $HANDOFF_FILE with a concise continuation handoff. Include the objective, constraints, completed work, repository and git state, files changed, architectural decisions, tests and results, unresolved problems, and exact next steps. Do not perform further implementation in this turn."

        run_codex codex exec resume --json "$thread_id" "$handoff_prompt"
        status=$?

        if usage_exhausted; then
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
