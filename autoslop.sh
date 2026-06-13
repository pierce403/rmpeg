#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

PROMPT="${AUTOSLOP_PROMPT:-agents/autoslop.md}"
MIN_STRICT_GAIN="${AUTOSLOP_MIN_STRICT_GAIN:-1}"
MIN_PERCENT_GAIN="${AUTOSLOP_MIN_PERCENT_GAIN:-1.0}"
COMMIT_MESSAGE="${AUTOSLOP_COMMIT_MESSAGE:-autoslop: improve sample media progress}"
PUSH_AFTER_COMMIT="${AUTOSLOP_PUSH:-0}"
RUN_ID="$(date -u +%Y%m%d-%H%M%S)"
RUN_LOG="agents/runs/autoslop-${RUN_ID}.md"
CORRECTNESS="site/data/correctness.json"
UPSTREAM="site/data/upstream-samples.json"

usage() {
  cat <<'EOF'
usage: ./autoslop.sh [--help]

Runs one autonomous coding attempt with guardrails based on rmpeg Phase 1 metrics.

Agent selection:
  AGENT_CMD="codex exec --full-auto agents/autoslop.md" ./autoslop.sh

If AGENT_CMD is unset, autoslop tries known local harnesses in this order:
  codex, claude, aider

Required local tools:
  git, python3, cargo, ffprobe, ffmpeg, hyperfine

Guardrail environment:
  AUTOSLOP_MIN_STRICT_GAIN=1        minimum additional strict media matches
  AUTOSLOP_MIN_PERCENT_GAIN=1.0     minimum absolute Phase 1 percent gain
  AUTOSLOP_PROMPT=agents/autoslop.md
  AUTOSLOP_COMMIT_MESSAGE="autoslop: improve sample media progress"
  AUTOSLOP_PUSH=1                  push the kept commit to origin/main
  RMPEG_FFMPEG_SAMPLE_LIMIT=100     optional smoke-test limit for local debugging

The coding harness starts only after the baseline strict media metric is printed.
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

say() {
  printf '[autoslop] %s\n' "$*"
}

die() {
  printf '[autoslop] error: %s\n' "$*" >&2
  exit 2
}

require_tool() {
  command -v "$1" >/dev/null 2>&1 || die "required tool missing: $1"
}

require_clean_tree() {
  if [[ -n "$(git status --porcelain --untracked-files=all)" ]]; then
    git status --short
    die "working tree must be clean before autoslop can safely reset failed attempts"
  fi
}

resolve_agent_cmd() {
  if [[ -n "${AGENT_CMD:-}" ]]; then
    printf '%s\n' "$AGENT_CMD"
    return
  fi

  local prompt_q
  prompt_q="$(printf '%q' "$PROMPT")"
  if command -v codex >/dev/null 2>&1; then
    printf 'codex exec --full-auto %s\n' "$prompt_q"
    return
  fi
  if command -v claude >/dev/null 2>&1; then
    printf 'claude -p "$(cat %s)"\n' "$prompt_q"
    return
  fi
  if command -v aider >/dev/null 2>&1; then
    printf 'aider --yes --message-file %s\n' "$prompt_q"
    return
  fi

  die "no coding harness found; set AGENT_CMD explicitly"
}

run_cmd() {
  local start status elapsed
  start="$SECONDS"
  say "+ $*"
  set +e
  "$@"
  status=$?
  set -e
  elapsed=$((SECONDS - start))
  if [[ "$status" -eq 0 ]]; then
    say "ok (${elapsed}s): $*"
  else
    say "failed (${elapsed}s, exit ${status}): $*"
  fi
  return "$status"
}

run_baseline() {
  say "baseline stage 1/4: generated local fixtures"
  run_cmd cargo xtask samples
  say "baseline stage 2/4: FFmpeg references for mirrored tests"
  run_cmd cargo xtask reference
  say "baseline stage 3/4: mirrored mini-suite"
  run_cmd cargo xtask fate-mini
  say "baseline stage 4/4: upstream FFmpeg sample corpus; the coding harness has not started yet"
  run_cmd cargo xtask ffmpeg-samples-check
}

run_after_gate() {
  say "guardrail stage 1/9: formatting"
  run_cmd cargo fmt --check &&
    say "guardrail stage 2/9: clippy" &&
    run_cmd cargo clippy --workspace -- -D warnings &&
    say "guardrail stage 3/9: Rust tests" &&
    run_cmd cargo test --workspace &&
    say "guardrail stage 4/9: generated local fixtures" &&
    run_cmd cargo xtask samples &&
    say "guardrail stage 5/9: FFmpeg references" &&
    run_cmd cargo xtask reference &&
    say "guardrail stage 6/9: mirrored mini-suite" &&
    run_cmd cargo xtask fate-mini &&
    say "guardrail stage 7/9: upstream FFmpeg sample corpus" &&
    run_cmd cargo xtask ffmpeg-samples-check &&
    say "guardrail stage 8/9: benchmarks" &&
    run_cmd cargo xtask bench &&
    say "guardrail stage 9/9: generated site" &&
    run_cmd cargo xtask site
}

metrics() {
  python3 - "$CORRECTNESS" "$UPSTREAM" <<'PY'
import json
import sys
from pathlib import Path

correctness = json.loads(Path(sys.argv[1]).read_text())
upstream = json.loads(Path(sys.argv[2]).read_text())
tests = upstream.get("tests", [])
summary = upstream.get("summary", {})

strict = sum(
    1 for test in tests
    if test.get("ffprobe_returncode") == 0 and test.get("status") == "passed"
)
accepted = int(summary.get("ffprobe_accepted") or 0)
percent = strict / accepted * 100 if accepted else 0.0
total_passed = int(summary.get("passed") or 0)
errors = int(summary.get("errors") or 0)
false_accepts = sum(
    1 for test in tests
    if test.get("ffprobe_returncode") != 0 and test.get("rmpeg_returncode") == 0
)
weights = {"passed": 10, "failed": -20, "error": -20, "skipped": -10}
mirrored_score = sum(weights[test["status"]] for test in correctness.get("tests", []))
print(strict, accepted, f"{percent:.6f}", total_passed, errors, false_accepts, mirrored_score)
PY
}

changed_files() {
  git status --porcelain --untracked-files=all | sed 's/^...//' | sed 's/.* -> //'
}

forbidden_changed_files() {
  local path
  while IFS= read -r path; do
    case "$path" in
      harness/reference/*|harness/scripts/compare_*|harness/scripts/score.py|.github/workflows/*|site/data/*|site/dist/*)
        printf '%s\n' "$path"
        ;;
    esac
  done
}

write_log() {
  local status="$1"
  local reason="$2"
  mkdir -p agents/runs
  {
    printf '# autoslop %s\n\n' "$RUN_ID"
    printf 'status: %s\n\n' "$status"
    printf -- '- reason: %s\n' "$reason"
    printf -- '- original_commit: %s\n' "$ORIGINAL_COMMIT"
    printf -- '- agent_command: `%s`\n' "$AGENT_COMMAND"
    printf -- '- agent_exit_code: %s\n' "$AGENT_STATUS"
    printf -- '- gate_exit_code: %s\n' "$GATE_STATUS"
    printf -- '- baseline_strict: %s / %s (%s%%)\n' "$BASE_STRICT" "$BASE_ACCEPTED" "$BASE_PERCENT"
    printf -- '- after_strict: %s / %s (%s%%)\n' "$AFTER_STRICT" "$AFTER_ACCEPTED" "$AFTER_PERCENT"
    printf -- '- baseline_total_passed: %s\n' "$BASE_TOTAL_PASSED"
    printf -- '- after_total_passed: %s\n' "$AFTER_TOTAL_PASSED"
    printf -- '- baseline_false_accepts: %s\n' "$BASE_FALSE_ACCEPTS"
    printf -- '- after_false_accepts: %s\n' "$AFTER_FALSE_ACCEPTS"
    printf -- '- baseline_mirrored_score: %s\n' "$BASE_MIRRORED_SCORE"
    printf -- '- after_mirrored_score: %s\n' "$AFTER_MIRRORED_SCORE"
    if [[ -n "${FORBIDDEN_FILES:-}" ]]; then
      printf -- '- forbidden_files: %s\n' "$FORBIDDEN_FILES"
    fi
  } >"$RUN_LOG"
}

reset_attempt() {
  say "reverting attempt to $ORIGINAL_COMMIT"
  git reset --hard "$ORIGINAL_COMMIT"
  git clean -fd
}

should_keep() {
  python3 - "$BASE_STRICT" "$AFTER_STRICT" "$BASE_PERCENT" "$AFTER_PERCENT" \
    "$MIN_STRICT_GAIN" "$MIN_PERCENT_GAIN" "$BASE_FALSE_ACCEPTS" "$AFTER_FALSE_ACCEPTS" \
    "$AFTER_ERRORS" "$BASE_MIRRORED_SCORE" "$AFTER_MIRRORED_SCORE" <<'PY'
import sys

base_strict = int(sys.argv[1])
after_strict = int(sys.argv[2])
base_percent = float(sys.argv[3])
after_percent = float(sys.argv[4])
min_strict = int(sys.argv[5])
min_percent = float(sys.argv[6])
base_false = int(sys.argv[7])
after_false = int(sys.argv[8])
after_errors = int(sys.argv[9])
base_mirror = int(sys.argv[10])
after_mirror = int(sys.argv[11])

reasons = []
if after_strict - base_strict < min_strict:
    reasons.append(f"strict gain {after_strict - base_strict} < {min_strict}")
if after_percent - base_percent < min_percent:
    reasons.append(f"percent gain {after_percent - base_percent:.3f} < {min_percent:.3f}")
if after_false > base_false:
    reasons.append(f"false accepts increased {base_false} -> {after_false}")
if after_errors != 0:
    reasons.append(f"corpus errors = {after_errors}")
if after_mirror < base_mirror:
    reasons.append(f"mirrored score decreased {base_mirror} -> {after_mirror}")

if reasons:
    print("no\t" + "; ".join(reasons))
else:
    print("yes\tguardrails passed")
PY
}

require_tool git
require_tool python3
require_tool cargo
require_tool ffprobe
require_tool ffmpeg
require_tool hyperfine
[[ -f "$PROMPT" ]] || die "missing prompt file: $PROMPT"
require_clean_tree

ORIGINAL_COMMIT="$(git rev-parse HEAD)"
AGENT_COMMAND="$(resolve_agent_cmd)"
AGENT_STATUS=999
GATE_STATUS=999
FORBIDDEN_FILES=""

say "baseline from $ORIGINAL_COMMIT"
say "selected coding harness command: $AGENT_COMMAND"
say "first measuring baseline metrics; the coding harness starts after the baseline strict media line"
if [[ -n "${RMPEG_FFMPEG_SAMPLE_LIMIT:-}" ]]; then
  say "using RMPEG_FFMPEG_SAMPLE_LIMIT=$RMPEG_FFMPEG_SAMPLE_LIMIT for the upstream corpus check"
else
  say "using full upstream corpus; ffmpeg-samples-check can take several minutes"
fi
run_baseline
read -r BASE_STRICT BASE_ACCEPTED BASE_PERCENT BASE_TOTAL_PASSED BASE_ERRORS BASE_FALSE_ACCEPTS BASE_MIRRORED_SCORE < <(metrics)
say "baseline strict media: $BASE_STRICT / $BASE_ACCEPTED (${BASE_PERCENT}%)"

say "running agent: $AGENT_COMMAND"
set +e
bash -lc "$AGENT_COMMAND"
AGENT_STATUS=$?
set -e

say "running guardrail gate"
set +e
run_after_gate
GATE_STATUS=$?
set -e

if [[ "$GATE_STATUS" -eq 0 ]]; then
  read -r AFTER_STRICT AFTER_ACCEPTED AFTER_PERCENT AFTER_TOTAL_PASSED AFTER_ERRORS AFTER_FALSE_ACCEPTS AFTER_MIRRORED_SCORE < <(metrics)
else
  AFTER_STRICT="$BASE_STRICT"
  AFTER_ACCEPTED="$BASE_ACCEPTED"
  AFTER_PERCENT="$BASE_PERCENT"
  AFTER_TOTAL_PASSED="$BASE_TOTAL_PASSED"
  AFTER_ERRORS="$BASE_ERRORS"
  AFTER_FALSE_ACCEPTS="$BASE_FALSE_ACCEPTS"
  AFTER_MIRRORED_SCORE="$BASE_MIRRORED_SCORE"
fi

FORBIDDEN_FILES="$(changed_files | forbidden_changed_files | paste -sd ', ' -)"
read -r KEEP DECISION_REASON < <(should_keep)

if [[ "$AGENT_STATUS" -ne 0 ]]; then
  KEEP="no"
  DECISION_REASON="agent exited with $AGENT_STATUS"
elif [[ "$GATE_STATUS" -ne 0 ]]; then
  KEEP="no"
  DECISION_REASON="guardrail gate exited with $GATE_STATUS"
elif [[ -n "$FORBIDDEN_FILES" ]]; then
  KEEP="no"
  DECISION_REASON="forbidden files changed"
fi

if [[ "$KEEP" == "yes" ]]; then
  write_log "kept" "$DECISION_REASON"
  run_cmd git add -A
  run_cmd git commit -m "$COMMIT_MESSAGE"
  if [[ "$PUSH_AFTER_COMMIT" == "1" ]]; then
    run_cmd git push origin main
  fi
  say "kept: $BASE_STRICT -> $AFTER_STRICT strict media matches"
else
  reset_attempt
  write_log "reverted" "$DECISION_REASON"
  say "reverted: $DECISION_REASON"
  exit 1
fi
