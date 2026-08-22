#!/usr/bin/env bash

set -Eeuo pipefail
IFS=$'\n\t'

# Repeatedly run Codex until account usage is exhausted. Each successful
# implementation turn is a commit boundary owned by this script.
#
# Usage:
#   ./codex-until-limit.sh --mode backlog "Implement the remaining project backlog"
#
# Configuration:
#   CODEX_CONTEXT_WINDOW=128000      Fallback model context window in tokens
#   CODEX_MIN_CONTEXT_PERCENT=20     Start a fresh conversation at this remainder
#   CODEX_WORKDIR="$PWD"             Repository Codex should work in
#   CODEX_HANDOFF_FILE                Handoff path; defaults outside the repository
#   CODEX_LOOP_LOG                    Log path; defaults outside the repository
#   CODEX_REFACTOR_RULES_FILE         Rules path (default: docs/REFACTORING-RULES.md)

usage() {
    cat <<'EOF'
Usage: ./codex-until-limit.sh [options] [TASK]

Continuously runs Codex until usage is exhausted, a stop file is created, or
the loop cannot safely continue. Every successful turn is committed before the
next turn starts.

Modes:
  feature                    Implement one coherent increment of TASK (default)
  backlog                    Implement and finish one unchecked backlog item per turn
  refactor                   Audit the codebase by its existing sections, write
                             refactoring rules, then implement one scoped increment

Options:
  --mode MODE                Select feature, backlog, or refactor
  --max-turns N              Stop after N committed turns
  --once                     Alias for --max-turns 1
  --allow-dirty              Allow an existing worktree; it may be included in
                             the first automatic commit, so use only to resume work
  --dry-run                  Show the selected mode and next prompt without running Codex
  -h, --help                 Show this help

Examples:
  ./codex-until-limit.sh --mode backlog --once
  ./codex-until-limit.sh --mode refactor
  ./codex-until-limit.sh --mode feature "Implement the next documented increment"

Set CODEX_CONTEXT_WINDOW to the context-window value shown by Codex /status.
EOF
}

die() {
    printf 'codex-until-limit: %s\n' "$*" >&2
    exit 1
}

MODE=feature
MAX_TURNS=0
ALLOW_DIRTY=0
DRY_RUN=0
TASK_ARGS=()

while (($# > 0)); do
    case "$1" in
        --mode)
            (($# >= 2)) || die "--mode requires feature, backlog, or refactor"
            MODE=$2
            shift 2
            ;;
        --max-turns)
            (($# >= 2)) || die "--max-turns requires a positive integer"
            MAX_TURNS=$2
            [[ "$MAX_TURNS" =~ ^[1-9][0-9]*$ ]] || die "--max-turns must be a positive integer"
            shift 2
            ;;
        --once)
            MAX_TURNS=1
            shift
            ;;
        --allow-dirty)
            ALLOW_DIRTY=1
            shift
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        --)
            shift
            while (($# > 0)); do
                TASK_ARGS+=("$1")
                shift
            done
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            TASK_ARGS+=("$1")
            shift
            ;;
    esac
done

case "$MODE" in
    feature|backlog|refactor)
        ;;
    *)
        die "unknown mode '$MODE'; expected feature, backlog, or refactor"
        ;;
esac

CODEX_BIN=${CODEX_BIN:-codex}
WORKDIR=${CODEX_WORKDIR:-$PWD}
if [[ -n "${CODEX_CONTEXT_WINDOW:-}" ]]; then
    CONTEXT_WINDOW_EXPLICIT=1
else
    CONTEXT_WINDOW_EXPLICIT=0
fi
CONTEXT_WINDOW="${CODEX_CONTEXT_WINDOW:-128000}"
MIN_CONTEXT_PERCENT="${CODEX_MIN_CONTEXT_PERCENT:-20}"

[[ "$CONTEXT_WINDOW" =~ ^[1-9][0-9]*$ ]] || die "CODEX_CONTEXT_WINDOW must be a positive integer"
[[ "$MIN_CONTEXT_PERCENT" =~ ^[0-9]+$ ]] \
    && ((MIN_CONTEXT_PERCENT >= 1 && MIN_CONTEXT_PERCENT <= 99)) \
    || die "CODEX_MIN_CONTEXT_PERCENT must be an integer from 1 to 99"
[[ -d "$WORKDIR" ]] || die "work directory does not exist: $WORKDIR"

for command in "$CODEX_BIN" jq git; do
    command -v "$command" >/dev/null 2>&1 || die "required command not found: $command"
done

REPO_ROOT=$(git -C "$WORKDIR" rev-parse --show-toplevel 2>/dev/null) \
    || die "CODEX_WORKDIR is not inside a Git repository: $WORKDIR"
RULES_FILE=${CODEX_REFACTOR_RULES_FILE:-$REPO_ROOT/docs/REFACTORING-RULES.md}

if [[ -n "${CODEX_HANDOFF_FILE:-}" ]]; then
    HANDOFF_FILE=$CODEX_HANDOFF_FILE
else
    HANDOFF_FILE="${TMPDIR:-/tmp}/mesh-codex-continuation.md"
fi
if [[ -n "${CODEX_LOOP_LOG:-}" ]]; then
    LOG_FILE=$CODEX_LOOP_LOG
else
    LOG_FILE="${TMPDIR:-/tmp}/mesh-codex-loop.log"
fi
[[ "$HANDOFF_FILE" = /* ]] || HANDOFF_FILE="$WORKDIR/$HANDOFF_FILE"
[[ "$LOG_FILE" = /* ]] || LOG_FILE="$WORKDIR/$LOG_FILE"
STOP_FILE="$WORKDIR/.codex-loop.stop"

if ((${#TASK_ARGS[@]} > 0)); then
    TASK="${TASK_ARGS[*]}"
else
    TASK="Implement one coherent, production-quality increment from the documented MESH work. Inspect the current repository state, run focused validation, and stop after this increment is complete."
fi

next_backlog_line() {
    awk '/^- \[ \] / { print NR; exit }' "$REPO_ROOT/docs/BACKLOG.md"
}

backlog_item_at() {
    local line=$1
    sed -n "${line}p" "$REPO_ROOT/docs/BACKLOG.md"
}

assert_git_state() {
    local branch
    branch=$(git -C "$REPO_ROOT" symbolic-ref --short -q HEAD || true)
    [[ -n "$branch" ]] || die "refusing to run from a detached HEAD"

    if [[ -n "$(git -C "$REPO_ROOT" status --porcelain=v1 --untracked-files=all)" ]]; then
        git -C "$REPO_ROOT" status --short >&2
        if ((ALLOW_DIRTY)); then
            printf 'codex-until-limit: continuing with existing changes because --allow-dirty was supplied\n' >&2
        else
            die "working tree is not clean; commit or set aside existing changes first (or pass --allow-dirty to resume)"
        fi
    fi
}

print_configuration() {
    local rollover_at=$((CONTEXT_WINDOW * (100 - MIN_CONTEXT_PERCENT) / 100))
    printf 'repository: %s\n' "$REPO_ROOT"
    printf 'branch: %s\n' "$(git -C "$REPO_ROOT" branch --show-current)"
    printf 'mode: %s\n' "$MODE"
    printf 'context window: %s tokens\n' "$CONTEXT_WINDOW"
    printf 'fresh-session threshold: %s tokens used (%s%% remaining)\n' "$rollover_at" "$MIN_CONTEXT_PERCENT"
    printf 'automatic commits: enabled (operational log and handoff stay outside feature commits)\n'
    printf 'allow dirty recovery: %s\n' "$ALLOW_DIRTY"
}

build_refactor_audit_prompt() {
    cat <<EOF
You are the audit/planning turn of the MESH refactoring loop.

The goal is to keep a large Rust/Wayland/Luau codebase clean while refactoring
it safely. Read AGENTS.md, docs/architecture/overview.md, docs/spec/README.md,
.planning/README.md, and especially .planning/log/sections.md. Treat the
existing package-oriented sections as the initial map; verify them against the
actual source files instead of trusting stale documentation.

Inspect every section's listed source roots and its existing improvements.md.
Look for misplaced ownership, dependency-direction violations, duplicated
contracts, overly broad modules, service-specific policy in generic core code,
unsafe or lossy boundaries, and tests that do not protect the seams. Build a
concise source-grounded set of refactoring rules in:

  $RULES_FILE

The rules document must describe section ownership, allowed dependency flow,
when to split a module, what belongs in Rust versus Luau/modules, required
contract/test boundaries, and a practical checklist for future changes. Include
the evidence paths that led to each rule. Rules are durable guidance, not a
second backlog or a dated progress tracker; open work still belongs in
docs/BACKLOG.md and historical findings stay in .planning/log/.

This turn is analysis and documentation only. Do not refactor production code,
do not create a competing task list, do not edit .planning/archive/, and do not
run broad speculative rewrites. The loop will commit this rules document after
the turn.
EOF
}

build_prompt() {
    local line item

    case "$MODE" in
        feature)
            cat <<EOF
You are one worker in the MESH implementation loop.

Implement exactly one coherent increment for this task:

$TASK

Read AGENTS.md, docs/architecture/overview.md, docs/spec/README.md,
docs/BACKLOG.md, .planning/STATUS.md, .planning/README.md, and the relevant
source, tests, history, and planning records before editing. Preserve the
documented architecture and terminology. Run focused tests and the broadest
practical validation. Do not start unrelated work.

Do not create a Git commit; the outer loop creates one commit for this turn.
Leave the repository in a reviewable state and report the files changed, tests
run, and any unresolved limitation in your final response.
EOF
            ;;
        backlog)
            line=$(next_backlog_line)
            [[ -n "$line" ]] || return 1
            item=$(backlog_item_at "$line")
            cat <<EOF
You are one worker in the MESH backlog implementation loop.

Implement exactly this unchecked item from docs/BACKLOG.md:

$item

Before editing, read AGENTS.md, docs/architecture/overview.md,
docs/spec/README.md, docs/BACKLOG.md, .planning/STATUS.md,
.planning/README.md, and the relevant audit/log files. Re-read the backlog
before deciding because earlier work may have shifted it. Implement only this
item, run focused tests and the broadest practical validation, delete the item
from docs/BACKLOG.md only when it is genuinely complete, append its required
dated log record, and update .planning/STATUS.md only if the in-flight work
changed. Do not edit .planning/archive/ or start another item.

Do not create a Git commit; the outer loop creates exactly one commit for this
turn. If the item is blocked, make no fake completion and leave useful
diagnostics instead of removing it.
EOF
            ;;
        refactor)
            if [[ ! -s "$RULES_FILE" ]]; then
                build_refactor_audit_prompt
            else
                cat <<EOF
You are one worker in the MESH section-by-section refactoring loop.

Read AGENTS.md, docs/architecture/overview.md, docs/spec/README.md,
.planning/README.md, .planning/log/sections.md, and $RULES_FILE. Use the
existing section map and audit files as evidence, then inspect the current
source before acting. Choose the single highest-value safe increment in one
section. Improve ownership, dependency direction, contract clarity, or test
coverage according to the rules. Keep public behavior stable unless the
change is explicitly required by the relevant spec/backlog item.

Implement only that one scoped increment. If it is a new backlog item, add it
to docs/BACKLOG.md before starting and remove it only when complete; otherwise
do not invent a second tracker. Run focused validation and record real failures
in the normal planning log. Do not edit .planning/archive/.

Do not create a Git commit; the outer loop creates one commit for this turn.
Report the section, files changed, tests run, and the next safe increment.
EOF
            fi
            ;;
    esac
}

build_continuation_prompt() {
    case "$MODE" in
        feature)
            printf 'Continue the feature-loop task. Inspect the current repository state, complete exactly one additional coherent increment of this task, run relevant tests, and do not commit because the outer loop commits each turn. Task: %s' "$TASK"
            ;;
        backlog)
            printf 'Continue backlog mode. Re-read docs/BACKLOG.md, select its next unchecked item, implement only that item, run relevant tests, update required tracking files when complete, and do not commit because the outer loop commits each turn.'
            ;;
        refactor)
            if [[ ! -s "$RULES_FILE" ]]; then
                build_refactor_audit_prompt
            else
                printf 'Continue the MESH refactoring loop. Read the current section map and %s, choose one safe scoped increment in one section, implement it, run relevant tests, and do not commit because the outer loop commits each turn.' "$RULES_FILE"
            fi
            ;;
    esac
}

if ((DRY_RUN)); then
    print_configuration
    if [[ "$MODE" == backlog && -z "$(next_backlog_line)" ]]; then
        printf 'next prompt: no unchecked backlog items remain\n'
    else
        printf '\n--- next prompt ---\n'
        build_prompt
    fi
    exit 0
fi

assert_git_state

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

    mkdir -p -- "$(dirname -- "$LOG_FILE")"
    touch "$LOG_FILE"
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

read_thread_id() {
    jq -r 'select(.type == "thread.started") | .thread_id // empty' "$TURN_JSON" | head -n 1
}

git_pathspec_exclusions() {
    local path relative
    # These files are loop state, not implementation work. Keep the old
    # repository-local names excluded for compatibility, while also excluding
    # custom paths when a caller intentionally places them inside the repo.
    printf '%s\n' \
        ':(exclude)codex-loop.log' \
        ':(exclude).codex-continuation.md' \
        ':(exclude).codex-loop.stop'
    for path in "$LOG_FILE" "$HANDOFF_FILE" "$STOP_FILE"; do
        case "$path" in
            "$REPO_ROOT"/*)
                relative=${path#"$REPO_ROOT"/}
                printf ':(exclude)%s\n' "$relative"
                ;;
        esac
    done
}

has_feature_changes() {
    local -a exclusions
    mapfile -t exclusions < <(git_pathspec_exclusions)
    if ! git -C "$REPO_ROOT" diff --quiet -- . "${exclusions[@]}"; then
        return 0
    fi
    if ! git -C "$REPO_ROOT" diff --cached --quiet -- . "${exclusions[@]}"; then
        return 0
    fi
    [[ -n "$(git -C "$REPO_ROOT" ls-files --others --exclude-standard -- . "${exclusions[@]}")" ]]
}

commit_subject() {
    local turn=$1 marker=${2:-}
    case "$MODE" in
        feature)
            printf 'codex: feature turn %s' "$turn"
            ;;
        backlog)
            marker=${marker#- \[ \] }
            marker=$(printf '%s' "$marker" | tr '\n' ' ' | sed 's/[[:space:]][[:space:]]*/ /g' | cut -c1-60)
            if [[ -n "$marker" ]]; then
                printf 'codex: backlog - %s' "$marker"
            else
                printf 'codex: backlog turn %s' "$turn"
            fi
            ;;
        refactor)
            if ((REFRACTOR_AUDIT_TURN)); then
                printf 'codex: refactor audit rules'
            else
                printf 'codex: refactor section increment %s' "$turn"
            fi
            ;;
    esac
}

commit_turn() {
    local before=$1 turn=$2 marker=${3:-}
    local after commit_count parent_count

    after=$(git -C "$REPO_ROOT" rev-parse HEAD)
    git -C "$REPO_ROOT" merge-base --is-ancestor "$before" "$after" \
        || die "history was rewritten during the Codex turn"
    commit_count=$(git -C "$REPO_ROOT" rev-list --count "$before..$after")
    ((commit_count <= 1)) || die "Codex created more than one commit in a turn"

    if [[ "$MODE" == backlog && -n "$marker" ]] \
        && grep -Fq -- "$marker" "$REPO_ROOT/docs/BACKLOG.md"; then
        die "backlog item was not completed; leaving Codex changes uncommitted for review"
    fi
    if [[ "$MODE" == refactor && ! -s "$RULES_FILE" ]]; then
        die "refactor audit did not create a non-empty $RULES_FILE"
    fi

    if ((commit_count == 1)); then
        if has_feature_changes; then
            die "Codex created a commit but left additional implementation changes uncommitted"
        fi
    else
        local -a exclusions
        mapfile -t exclusions < <(git_pathspec_exclusions)
        git -C "$REPO_ROOT" add -A -- . "${exclusions[@]}"
        if ! git -C "$REPO_ROOT" diff --cached --quiet; then
            git -C "$REPO_ROOT" commit -m "$(commit_subject "$turn" "$marker")"
        else
            printf 'No implementation changes were produced in turn %s; stopping to avoid a spin loop.\n' "$turn" >&2
            return 2
        fi
    fi

    after=$(git -C "$REPO_ROOT" rev-parse HEAD)
    parent_count=$(git -C "$REPO_ROOT" rev-list --parents -n 1 "$after" | awk '{ print NF - 1 }')
    [[ "$parent_count" == 1 ]] || die "the feature commit must not be a merge commit"
    printf '%s\n' "$after"
}

if ((DRY_RUN)); then
    print_configuration
    if [[ "$MODE" == backlog && -z "$(next_backlog_line)" ]]; then
        printf 'next prompt: no unchecked backlog items remain\n'
    else
        printf '\n--- next prompt ---\n'
        build_prompt
    fi
    exit 0
fi

assert_git_state

thread_id=''
committed_turns=0
failures=0
REFRACTOR_AUDIT_TURN=0

printf 'Starting Codex loop in %s (mode: %s)\n' "$REPO_ROOT" "$MODE" | tee -a "$LOG_FILE"
printf 'Fallback context rollover threshold: %s / %s tokens used\n' \
    "$FALLBACK_ROLLOVER_AT" "$CONTEXT_WINDOW" | tee -a "$LOG_FILE"
printf 'Automatic commit boundary: enabled\n' | tee -a "$LOG_FILE"
printf 'Create %s to stop after the current turn.\n' "$STOP_FILE" | tee -a "$LOG_FILE"

while [[ ! -e "$STOP_FILE" ]]; do
    if ((MAX_TURNS > 0 && committed_turns >= MAX_TURNS)); then
        printf 'Reached --max-turns=%s.\n' "$MAX_TURNS"
        exit 0
    fi

    if [[ "$MODE" == backlog && -z "$(next_backlog_line)" ]]; then
        printf 'No unchecked backlog items remain.\n'
        exit 0
    fi

    before=$(git -C "$REPO_ROOT" rev-parse HEAD)
    marker=''
    if [[ "$MODE" == backlog ]]; then
        marker=$(backlog_item_at "$(next_backlog_line)")
    fi
    if [[ "$MODE" == refactor && ! -s "$RULES_FILE" ]]; then
        REFRACTOR_AUDIT_TURN=1
    else
        REFRACTOR_AUDIT_TURN=0
    fi

    if [[ -z "$thread_id" ]]; then
        prompt=$(build_prompt) || {
            [[ "$MODE" == backlog ]] && printf 'No unchecked backlog items remain.\n'
            exit 0
        }
        command=(
            "$CODEX_BIN" exec
            --json
            --sandbox workspace-write
            -C "$WORKDIR"
            "$prompt"
        )
    else
        command=(
            "$CODEX_BIN" exec resume
            --json
            "$thread_id"
            "$(build_continuation_prompt)"
        )
    fi

    printf '\n=== turn %s (%s) ===\n' "$((committed_turns + 1))" "$MODE"
    if run_codex "${command[@]}"; then
        status=0
    else
        status=$?
        if usage_exhausted "$thread_id"; then
            printf 'Codex usage appears exhausted; stopping without committing an incomplete turn.\n' | tee -a "$LOG_FILE"
            exit 0
        fi
        ((failures += 1))
        printf 'Codex exited with status %s (%s/3).\n' "$status" "$failures" | tee -a "$LOG_FILE"
        if ((failures >= 3)); then
            exit "$status"
        fi
        sleep 60
        continue
    fi
    failures=0

    new_thread_id=$(read_thread_id)
    [[ -n "$thread_id" || -n "$new_thread_id" ]] || die "could not find the Codex thread ID in JSON output"
    [[ -n "$new_thread_id" ]] && thread_id=$new_thread_id

    if commit=$(commit_turn "$before" "$((committed_turns + 1))" "$marker"); then
        committed_turns=$((committed_turns + 1))
        printf 'Committed turn %s as %s\n' "$committed_turns" "$commit"
    else
        commit_status=$?
        ((commit_status == 2)) && exit 0
        exit "$commit_status"
    fi

    if usage_exhausted "$thread_id"; then
        printf 'Codex usage appears exhausted after the completed turn; stopping.\n' | tee -a "$LOG_FILE"
        exit 0
    fi

    context_usage_value=$(context_usage "$thread_id" || true)
    if [[ -z "$context_usage_value" ]]; then
        printf 'Current context usage was unavailable; starting a fresh conversation conservatively.\n' \
            | tee -a "$LOG_FILE"
        thread_id=''
        continue
    fi

    read -r used_tokens reported_context_window <<<"$context_usage_value"
    effective_context_window=$CONTEXT_WINDOW
    if (( !CONTEXT_WINDOW_EXPLICIT && reported_context_window > 0 )); then
        effective_context_window=$reported_context_window
    fi
    if ((effective_context_window <= 0)); then
        printf 'Invalid context window; starting a fresh conversation conservatively.\n' | tee -a "$LOG_FILE"
        thread_id=''
        continue
    fi

    if ((used_tokens >= effective_context_window)); then
        remaining_percent=0
    else
        remaining_percent=$(( (effective_context_window - used_tokens) * 100 / effective_context_window ))
        ((remaining_percent < 0)) && remaining_percent=0
        ((remaining_percent > 100)) && remaining_percent=100
    fi
    printf 'Estimated context usage: %s/%s tokens, %s%% remaining.\n' \
        "$used_tokens" "$effective_context_window" "$remaining_percent" | tee -a "$LOG_FILE"

    if ((remaining_percent <= MIN_CONTEXT_PERCENT)); then
        handoff_prompt="Context rollover is imminent. Write or replace $HANDOFF_FILE with a concise continuation handoff. Include the objective, mode, constraints, completed commits, repository and Git state, files changed, architectural decisions, tests and results, unresolved problems, and exact next steps. Do not perform further implementation in this turn and do not create a commit."

        if run_codex "$CODEX_BIN" exec resume --json "$thread_id" "$handoff_prompt"; then
            status=0
        else
            status=$?
            if usage_exhausted "$thread_id"; then
                printf 'Codex usage was exhausted while writing the handoff.\n' | tee -a "$LOG_FILE"
                exit 0
            fi
            printf 'Unable to create the context handoff; stopping safely.\n' >&2
            exit "$status"
        fi

        thread_id=''
        printf 'Handoff written; the next turn will use a fresh conversation.\n' | tee -a "$LOG_FILE"
    fi
done

printf 'Stop file detected; exiting.\n' | tee -a "$LOG_FILE"
