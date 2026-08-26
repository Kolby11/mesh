#!/usr/bin/env bash

# Run one Claude Code turn per backlog item. Every successful turn must leave
# one validated backlog commit on main, remove its backlog item, and add the
# required log record before the next item is attempted. Any additional
# worktree changes are preserved in a separate recovery commit.

set -Eeuo pipefail
IFS=$'\n\t'

usage() {
    cat <<'EOF'
Usage: scripts/claude-backlog-runner.sh [options]

Run Claude Code against the next unchecked item in docs/BACKLOG.md. The runner
requires a clean main branch by default and exactly one validated backlog
commit per item; extra changes are committed separately after validation.

Options:
  --dry-run                 Show the next item and configuration without running Claude
  --max-features N          Stop after N successful items (default: unlimited)
  --once                    Alias for --max-features 1
  --allow-dirty             Resume an interrupted turn with existing changes;
                            preserve them in a separate commit after success
  -h, --help                Show this help

Environment:
  CLAUDE_MODEL              Optional model override; otherwise Claude config applies
  CLAUDE_ALLOW_ALL          Set to 0 to use Claude's normal permission mode
                            (default: 1; unattended mode bypasses approvals)
  CLAUDE_AUTO_APPROVE       Set to 0 to leave permission prompts enabled when
                            CLAUDE_ALLOW_ALL=0 (default: 1)
  CLAUDE_WIFI_RECOVERY      Set to 0 to disable NetworkManager recovery
                            (default: 1)
  CLAUDE_WIFI_RECOVERY_COOLDOWN_SECONDS
                            Minimum seconds between recovery attempts (default: 30)
  CLAUDE_BIN                Claude executable (default: claude)
EOF
}

die() {
    printf 'claude-backlog-runner: %s\n' "$*" >&2
    exit 1
}

MAX_FEATURES=0
DRY_RUN=0
ALLOW_DIRTY=0

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
        --allow-dirty)
            ALLOW_DIRTY=1
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

CLAUDE_BIN=${CLAUDE_BIN:-claude}
CLAUDE_ALLOW_ALL=${CLAUDE_ALLOW_ALL:-1}
CLAUDE_AUTO_APPROVE=${CLAUDE_AUTO_APPROVE:-1}
CLAUDE_WIFI_RECOVERY=${CLAUDE_WIFI_RECOVERY:-1}
CLAUDE_WIFI_RECOVERY_COOLDOWN_SECONDS=${CLAUDE_WIFI_RECOVERY_COOLDOWN_SECONDS:-30}
CLAUDE_MODEL=${CLAUDE_MODEL:-}

[[ "$CLAUDE_ALLOW_ALL" == 0 || "$CLAUDE_ALLOW_ALL" == 1 ]] || die "CLAUDE_ALLOW_ALL must be 0 or 1"
[[ "$CLAUDE_AUTO_APPROVE" == 0 || "$CLAUDE_AUTO_APPROVE" == 1 ]] || die "CLAUDE_AUTO_APPROVE must be 0 or 1"
[[ "$CLAUDE_WIFI_RECOVERY" == 0 || "$CLAUDE_WIFI_RECOVERY" == 1 ]] || die "CLAUDE_WIFI_RECOVERY must be 0 or 1"
[[ "$CLAUDE_WIFI_RECOVERY_COOLDOWN_SECONDS" =~ ^[1-9][0-9]*$ ]] || die "CLAUDE_WIFI_RECOVERY_COOLDOWN_SECONDS must be a positive integer"

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(git -C "$SCRIPT_DIR/.." rev-parse --show-toplevel 2>/dev/null) || die "not inside a Git repository"
BACKLOG="$ROOT/docs/BACKLOG.md"
SCHEMA="$SCRIPT_DIR/claude-backlog-runner.schema.json"

[[ -f "$BACKLOG" ]] || die "missing $BACKLOG"
[[ -f "$SCHEMA" ]] || die "missing $SCHEMA"
if [[ "$CLAUDE_BIN" == */* ]]; then
    [[ -x "$CLAUDE_BIN" ]] || die "Claude executable not found or not executable: $CLAUDE_BIN"
else
    command -v "$CLAUDE_BIN" >/dev/null 2>&1 || die "Claude executable not found: $CLAUDE_BIN"
fi
command -v jq >/dev/null 2>&1 || die "jq is required to parse Claude JSONL output"
SCHEMA_CONTENT=$(<"$SCHEMA")

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
        if ((ALLOW_DIRTY)); then
            printf 'claude-backlog-runner: continuing with existing changes because --allow-dirty was supplied\n' >&2
        else
            die "working tree is not clean; commit or set aside existing changes first (or pass --allow-dirty to resume an interrupted turn)"
        fi
    fi
}

print_configuration() {
    printf 'repository: %s\n' "$ROOT"
    printf 'branch: %s\n' "$(git -C "$ROOT" branch --show-current)"
    printf 'permission bypass: %s\n' "$CLAUDE_ALLOW_ALL"
    printf 'auto approval: %s\n' "$CLAUDE_AUTO_APPROVE"
    printf 'allow dirty recovery: %s\n' "$ALLOW_DIRTY"
    printf 'Wi-Fi recovery: %s (cooldown %ss)\n' "$CLAUDE_WIFI_RECOVERY" "$CLAUDE_WIFI_RECOVERY_COOLDOWN_SECONDS"
    printf 'session policy: fresh Claude session per backlog item\n'
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

create_temp_dir() {
    local parent

    for parent in "${TMPDIR:-}" /tmp; do
        [[ -n "$parent" && -d "$parent" && -w "$parent" ]] || continue
        if TMP_DIR=$(mktemp -d "$parent/mesh-claude-runner.XXXXXX"); then
            if [[ -n "${TMPDIR:-}" && "$parent" != "$TMPDIR" ]]; then
                printf 'claude-backlog-runner: TMPDIR is unavailable; using %s\n' "$parent" >&2
            fi
            return 0
        fi
    done

    die "could not create a temporary directory; checked TMPDIR and /tmp"
}

TMP_DIR=''
create_temp_dir
trap 'rm -rf -- "$TMP_DIR"' EXIT

last_saved_wifi_uuid() {
    local profiles

    command -v nmcli >/dev/null 2>&1 || return 1
    profiles=$(nmcli -t -f UUID,TYPE,TIMESTAMP,AUTOCONNECT connection show 2>/dev/null) || return 1
    awk -F: '
        $2 == "802-11-wireless" {
            if ($3 ~ /^[0-9]+$/ && $3 + 0 > best_timestamp + 0) {
                best_timestamp = $3
                best_uuid = $1
            }
            if (fallback_uuid == "" && ($4 == "yes" || $4 == "true")) {
                fallback_uuid = $1
            }
        }
        END {
            if (best_uuid != "") {
                print best_uuid
            } else if (fallback_uuid != "") {
                print fallback_uuid
            }
        }
    ' <<<"$profiles"
}

recover_wifi() {
    ((CLAUDE_WIFI_RECOVERY)) || return 0

    if ! command -v nmcli >/dev/null 2>&1; then
        printf '[wifi] nmcli is unavailable; skipping Wi-Fi recovery\n' >&2
        return 0
    fi

    local now last_attempt='' wifi_uuid
    now=$(date +%s)
    if [[ -f "$TMP_DIR/wifi-recovery.last" ]]; then
        read -r last_attempt <"$TMP_DIR/wifi-recovery.last" || true
    fi
    if [[ "$last_attempt" =~ ^[0-9]+$ ]] \
        && ((now - last_attempt < CLAUDE_WIFI_RECOVERY_COOLDOWN_SECONDS)); then
        return 0
    fi
    printf '%s\n' "$now" >"$TMP_DIR/wifi-recovery.last"

    printf '[wifi] network wait detected; attempting saved Wi-Fi recovery\n' >&2
    if ! nmcli radio wifi on >/dev/null 2>&1; then
        printf '[wifi] could not access NetworkManager; skipping recovery\n' >&2
        return 0
    fi

    wifi_uuid=$(last_saved_wifi_uuid || true)
    if [[ -z "$wifi_uuid" ]]; then
        printf '[wifi] no saved Wi-Fi profile found; skipping recovery\n' >&2
        return 0
    fi

    if nmcli --wait 15 connection up uuid "$wifi_uuid" >/dev/null 2>&1; then
        printf '[wifi] requested the most recently used saved Wi-Fi profile\n' >&2
    else
        printf '[wifi] saved Wi-Fi profile is unavailable; continuing Claude retry\n' >&2
    fi
}

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
    local -a pipeline_status
    local result

    command=("$CLAUDE_BIN" --print --verbose --output-format stream-json --json-schema "$SCHEMA_CONTENT")
    if [[ -n "$CLAUDE_MODEL" ]]; then
        command+=(--model "$CLAUDE_MODEL")
    fi
    if ((CLAUDE_ALLOW_ALL)); then
        command+=(--dangerously-skip-permissions)
    elif ((CLAUDE_AUTO_APPROVE)); then
        command+=(--permission-mode acceptEdits)
    else
        command+=(--permission-mode manual)
    fi

    set +e
    "${command[@]}" "$prompt" 2>"$stderr_file" \
        | tee "$event_file" \
        | jq --unbuffered -r '
            def shorten:
                tostring
                | gsub("[\\r\\n\\t]"; " ")
                | if length > 180 then .[0:177] + "..." else . end;

            if .type == "system" and .subtype == "init" then
                "[claude] session started: \(.session_id)"
            elif .type == "assistant" then
                .message.content[]?
                | select(.type == "tool_use")
                | "[claude] running \(.name): \((.input.command // .input.cmd // .input | tojson) | shorten)"
            elif .type == "result" and (.is_error // false) then
                "[claude] error: \(.result // .subtype // "turn failed" | shorten)"
            elif .type == "result" then
                "[claude] turn completed"
            elif .type == "error" then
                "[claude] error: \(.error // .message // "unknown error" | shorten)"
            else
                empty
            end
        ' \
        | while IFS= read -r progress; do
            printf '%s\n' "$progress"
            if [[ "$progress" == "[claude] error:"* && "$progress" == *"waiting for network"* ]]; then
                recover_wifi
            fi
        done
    pipeline_status=("${PIPESTATUS[@]}")
    result=${pipeline_status[0]}
    set -e
    cat "$stderr_file" >&2
    return "$result"
}

read_final_response() {
    jq -r -s '
        map(select(.type == "result" and ((.is_error // false) | not)))
        | if length == 0 then ""
          else .[-1]
          | if .structured_output != null then .structured_output
            elif (.result | type) == "object" then .result
            elif (.result | type) == "string" then (.result | fromjson? // .)
            else null
            end
          | if . == null then ""
            elif type == "string" then .
            else tojson
            end
          end
    ' "$1"
}

commit_extra_worktree_changes() {
    local status

    status=$(git -C "$ROOT" status --short)
    [[ -n "$status" ]] || return 0

    printf 'Preserving additional worktree changes in a separate commit:\n%s\n' \
        "$status" >&2
    git -C "$ROOT" add -A
    if git -C "$ROOT" diff --cached --quiet; then
        return 0
    fi

    git -C "$ROOT" commit -m "claude: preserve additional worktree changes" >&2
    git -C "$ROOT" rev-parse HEAD
}

verify_feature_commit() {
    local before=$1
    local marker=$2
    local after commit_count parent_count primary_commit extra_commit

    after=$(git -C "$ROOT" rev-parse HEAD)
    primary_commit=$after
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

    if [[ -n "$(git -C "$ROOT" status --porcelain)" ]]; then
        extra_commit=$(commit_extra_worktree_changes)
        if [[ -n "$extra_commit" ]]; then
            printf 'Additional worktree changes committed as %s\n' "$extra_commit" >&2
        fi
    fi

    printf '%s\n' "$primary_commit"
}

successful_features=0

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
        die "Claude turn failed; inspect the repository and the captured session before retrying"
    fi

    final_response=$(read_final_response "$event_file")
    [[ -n "$final_response" ]] || die "Claude did not return its required structured result"
    jq -e 'type == "object" and .status == "complete"' >/dev/null <<<"$final_response" \
        || die "Claude did not report a complete feature result"
    printf 'Claude result: %s\n' "$final_response"

    commit=$(verify_feature_commit "$before" "$marker")
    successful_features=$((successful_features + 1))
    printf 'Committed feature %s as %s\n' "$successful_features" "$commit"
done
