#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

APPLY=0
LIMIT="${AUTOMERGE_LIMIT:-20}"
METHOD="${AUTOMERGE_METHOD:-squash}"
DELETE_BRANCH="${AUTOMERGE_DELETE_BRANCH:-1}"
REQUIRE_APPROVAL="${AUTOMERGE_REQUIRE_APPROVAL:-1}"
ALLOW_NO_CHECKS="${AUTOMERGE_ALLOW_NO_CHECKS:-0}"
ALLOW_PROTECTED="${AUTOMERGE_ALLOW_PROTECTED:-0}"
REQUIRE_LABEL="${AUTOMERGE_REQUIRE_LABEL:-}"
ALLOWED_STATES="${AUTOMERGE_ALLOWED_STATES:-CLEAN,HAS_HOOKS}"
REPO="${AUTOMERGE_REPO:-}"

usage() {
  cat <<'EOF'
usage: ./automerge.sh [--apply] [--limit N] [--method squash|merge|rebase]

Conservatively reviews open GitHub pull requests with gh. Dry-run by default.
Use --apply to merge candidates that satisfy all checks.

Environment:
  AUTOMERGE_LIMIT=20
  AUTOMERGE_METHOD=squash
  AUTOMERGE_DELETE_BRANCH=1
  AUTOMERGE_REQUIRE_APPROVAL=1
  AUTOMERGE_ALLOW_NO_CHECKS=0
  AUTOMERGE_ALLOWED_STATES=CLEAN,HAS_HOOKS
  AUTOMERGE_REQUIRE_LABEL=automerge
  AUTOMERGE_ALLOW_PROTECTED=0
  AUTOMERGE_REPO=owner/name
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --apply)
      APPLY=1
      shift
      ;;
    --limit)
      LIMIT="${2:?missing --limit value}"
      shift 2
      ;;
    --method)
      METHOD="${2:?missing --method value}"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      printf '[automerge] unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$METHOD" in
  squash|merge|rebase) ;;
  *) printf '[automerge] invalid method: %s\n' "$METHOD" >&2; exit 2 ;;
esac

command -v gh >/dev/null 2>&1 || {
  printf '[automerge] gh is required\n' >&2
  exit 2
}
command -v python3 >/dev/null 2>&1 || {
  printf '[automerge] python3 is required\n' >&2
  exit 2
}

GH_REPO_ARGS=()
if [[ -n "$REPO" ]]; then
  GH_REPO_ARGS=(--repo "$REPO")
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

prs_json="$tmpdir/prs.json"
candidates="$tmpdir/candidates.txt"

gh pr list "${GH_REPO_ARGS[@]}" \
  --state open \
  --limit "$LIMIT" \
  --json number,title,isDraft,mergeStateStatus,reviewDecision,statusCheckRollup,labels,url,author,baseRefName,headRefName \
  >"$prs_json"

python3 - "$prs_json" "$candidates" "$REQUIRE_APPROVAL" "$ALLOW_NO_CHECKS" "$REQUIRE_LABEL" "$ALLOWED_STATES" <<'PY'
import json
import sys
from pathlib import Path

prs = json.loads(Path(sys.argv[1]).read_text())
candidate_path = Path(sys.argv[2])
require_approval = sys.argv[3] != "0"
allow_no_checks = sys.argv[4] == "1"
require_label = sys.argv[5]
allowed_states = {item.strip() for item in sys.argv[6].split(",") if item.strip()}

candidate_numbers = []

def check_rollup_ok(rollup):
    if not rollup:
        return allow_no_checks, "no status checks"
    for check in rollup:
        status = str(check.get("status") or check.get("state") or "").upper()
        conclusion = str(check.get("conclusion") or check.get("state") or "").upper()
        name = check.get("name") or check.get("context") or "check"
        if status and status not in {"COMPLETED", "SUCCESS"}:
            return False, f"{name} is {status.lower()}"
        if conclusion and conclusion not in {"SUCCESS", "SKIPPED", "NEUTRAL"}:
            return False, f"{name} concluded {conclusion.lower()}"
    return True, "checks passed"

for pr in prs:
    number = pr["number"]
    title = pr["title"]
    reasons = []
    if pr.get("isDraft"):
        reasons.append("draft")
    merge_state = pr.get("mergeStateStatus") or ""
    if merge_state not in allowed_states:
        reasons.append(f"merge state {merge_state or 'unknown'}")
    review = pr.get("reviewDecision") or ""
    if require_approval and review != "APPROVED":
        reasons.append(f"review decision {review or 'none'}")
    labels = {label.get("name", "") for label in pr.get("labels", [])}
    if require_label and require_label not in labels:
        reasons.append(f"missing label {require_label}")
    checks_ok, checks_reason = check_rollup_ok(pr.get("statusCheckRollup") or [])
    if not checks_ok:
        reasons.append(checks_reason)

    if reasons:
        print(f"skip #{number}: {title} ({'; '.join(reasons)})")
    else:
        print(f"candidate #{number}: {title}")
        candidate_numbers.append(str(number))

candidate_path.write_text("\n".join(candidate_numbers) + ("\n" if candidate_numbers else ""))
PY

protected_files_for_pr() {
  local pr_number="$1"
  local files_json="$tmpdir/files-${pr_number}.json"
  gh pr view "$pr_number" "${GH_REPO_ARGS[@]}" --json files >"$files_json"
  python3 - "$files_json" <<'PY'
import fnmatch
import json
import sys
from pathlib import Path

patterns = [
    "harness/reference/**",
    "harness/scripts/compare_*",
    "harness/scripts/score.py",
    ".github/workflows/**",
    "site/data/**",
    "site/dist/**",
]
doc = json.loads(Path(sys.argv[1]).read_text())
for file in doc.get("files", []):
    path = file.get("path") or ""
    if any(fnmatch.fnmatch(path, pattern) for pattern in patterns):
        print(path)
PY
}

merge_pr() {
  local pr_number="$1"
  local args=(pr merge)
  args+=("$pr_number")
  args+=("${GH_REPO_ARGS[@]}")
  args+=("--$METHOD")
  if [[ "$DELETE_BRANCH" == "1" ]]; then
    args+=(--delete-branch)
  fi
  gh "${args[@]}"
}

merged=0
while IFS= read -r pr_number; do
  [[ -n "$pr_number" ]] || continue
  protected="$(protected_files_for_pr "$pr_number" | paste -sd ', ' -)"
  if [[ -n "$protected" && "$ALLOW_PROTECTED" != "1" ]]; then
    printf '[automerge] skip #%s: protected files changed: %s\n' "$pr_number" "$protected"
    continue
  fi

  if [[ "$APPLY" == "1" ]]; then
    printf '[automerge] merging #%s with %s\n' "$pr_number" "$METHOD"
    merge_pr "$pr_number"
  else
    printf '[automerge] dry-run would merge #%s with %s\n' "$pr_number" "$METHOD"
  fi
  merged=$((merged + 1))
done <"$candidates"

if [[ "$merged" -eq 0 ]]; then
  printf '[automerge] no merge candidates\n'
fi
