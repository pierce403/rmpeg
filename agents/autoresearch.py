#!/usr/bin/env python3
import fnmatch
import os
import shlex
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CORRECTNESS = ROOT / "site" / "data" / "correctness.json"
RUNS = ROOT / "agents" / "runs"
FORBIDDEN = [
    "harness/reference/**",
    "harness/scripts/compare_*",
    "harness/scripts/score.py",
    ".github/workflows/**",
    "site/data/**",
    "site/dist/**",
]


def main():
    agent_cmd = os.environ.get("AGENT_CMD")
    if not agent_cmd:
        print('AGENT_CMD is required, for example: AGENT_CMD="codex exec --full-auto agents/program.md"', file=sys.stderr)
        return 2

    started = timestamp()
    original = capture(["git", "rev-parse", "HEAD"]).strip()
    baseline_score = run_score("baseline")

    agent_result = subprocess.run(shlex.split(agent_cmd), cwd=ROOT)
    after_score = run_score("after-agent")
    changed = changed_files()
    forbidden = forbidden_files(changed)

    status = "reverted"
    details = []
    details.append(f"original: {original}")
    details.append(f"baseline_score: {baseline_score}")
    details.append(f"after_score: {after_score}")
    details.append(f"agent_exit_code: {agent_result.returncode}")

    if forbidden:
        details.append("forbidden_changes: " + ", ".join(forbidden))
        reset_to(original)
    elif agent_result.returncode == 0 and after_score > baseline_score:
        write_log(started, "improved", details)
        run(["git", "add", "-A"])
        run(["git", "commit", "-m", "autoresearch: improve mirrored test score"])
        status = "committed"
    else:
        reset_to(original)
        details.append("score did not improve")
        write_log(started, "reverted", details)

    print(f"autoresearch {status}: {baseline_score} -> {after_score}")
    return 0 if status == "committed" else 1


def run_score(label):
    print(f"running {label} score")
    run(["cargo", "xtask", "samples"])
    run(["cargo", "xtask", "reference"])
    run(["cargo", "xtask", "fate-mini"])
    output = capture(["python3", "harness/scripts/score.py", str(CORRECTNESS)])
    return int(output.strip())


def run(args):
    result = subprocess.run(args, cwd=ROOT)
    if result.returncode != 0:
        raise SystemExit(f"command failed: {' '.join(args)}")


def capture(args):
    result = subprocess.run(args, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode != 0:
        raise SystemExit(f"command failed: {' '.join(args)}\n{result.stderr}")
    return result.stdout


def changed_files():
    output = capture(["git", "status", "--porcelain", "--untracked-files=all"])
    files = []
    for line in output.splitlines():
        path = line[3:]
        if " -> " in path:
            path = path.split(" -> ", 1)[1]
        files.append(path)
    return files


def forbidden_files(files):
    bad = []
    for path in files:
        if any(fnmatch.fnmatch(path, pattern) for pattern in FORBIDDEN):
            bad.append(path)
    return bad


def reset_to(commit):
    subprocess.run(["git", "reset", "--hard", commit], cwd=ROOT, check=False)
    subprocess.run(["git", "clean", "-fd"], cwd=ROOT, check=False)


def write_log(started, status, details):
    RUNS.mkdir(parents=True, exist_ok=True)
    path = RUNS / f"{started}.md"
    lines = [f"# autoresearch {started}", "", f"status: {status}", ""]
    lines.extend(f"- {detail}" for detail in details)
    path.write_text("\n".join(lines) + "\n")


def timestamp():
    return datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S")


if __name__ == "__main__":
    raise SystemExit(main())
