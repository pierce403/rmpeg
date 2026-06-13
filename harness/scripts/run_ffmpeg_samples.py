#!/usr/bin/env python3
import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

from compare_probe_json import compare_probe, normalize_ffprobe_document

ROOT = Path(__file__).resolve().parents[2]
CACHE = ROOT / ".cache" / "ffmpeg"
DEFAULT_REPO = "https://git.ffmpeg.org/ffmpeg.git"
DEFAULT_REF = "master"
DEFAULT_SOURCE = CACHE / "src"
DEFAULT_BUILD = CACHE / "build"
DEFAULT_SAMPLES = CACHE / "fate-suite"
REPORT = ROOT / "site" / "data" / "upstream-samples.json"


def main():
    parser = argparse.ArgumentParser(
        description="Use upstream FFmpeg FATE sample scripts and probe the downloaded corpus."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("sync")
    check_parser = subparsers.add_parser("check")
    check_parser.add_argument(
        "--rmpeg-probe", default=str(ROOT / "target" / "release" / "rmpeg-probe")
    )
    check_parser.add_argument("--output", default=str(REPORT))
    subparsers.add_parser("all")
    args = parser.parse_args()

    if args.command == "sync":
        sync_samples()
        return 0
    elif args.command == "check":
        return check_samples(Path(args.rmpeg_probe), Path(args.output))
    sync_samples()
    return check_samples(ROOT / "target" / "release" / "rmpeg-probe", REPORT)


def sync_samples():
    require_tool("git")
    require_tool("make")
    require_tool("rsync")

    repo = os.environ.get("RMPEG_FFMPEG_REPO", DEFAULT_REPO)
    ref = os.environ.get("RMPEG_FFMPEG_REF", DEFAULT_REF)
    source = env_path("RMPEG_FFMPEG_SOURCE_DIR", DEFAULT_SOURCE)
    build = env_path("RMPEG_FFMPEG_BUILD_DIR", DEFAULT_BUILD)
    samples = env_path("RMPEG_FFMPEG_SAMPLES_DIR", DEFAULT_SAMPLES)

    source.parent.mkdir(parents=True, exist_ok=True)
    samples.mkdir(parents=True, exist_ok=True)
    checkout_ffmpeg(repo, ref, source)
    configure_ffmpeg(source, build, samples)

    command = ["make", "-C", str(build), "fate-rsync", f"SAMPLES={samples}"]
    rsync_options = os.environ.get("RMPEG_FATE_RSYNC_OPTIONS")
    dry_run = truthy(os.environ.get("RMPEG_FATE_RSYNC_DRY_RUN"))
    if dry_run:
        command.append("RSYNC_OPTIONS=-vrltLW --timeout=60 --dry-run")
    elif rsync_options:
        command.append(f"RSYNC_OPTIONS={rsync_options}")
    run(command, cwd=ROOT)
    action = "validated sync for" if dry_run else "synced"
    print(f"{action} FFmpeg FATE samples into {samples}")


def checkout_ffmpeg(repo, ref, source):
    if (source / ".git").exists():
        run(["git", "-C", str(source), "fetch", "--depth", "1", "origin", ref], cwd=ROOT)
        run(["git", "-C", str(source), "checkout", "--detach", "FETCH_HEAD"], cwd=ROOT)
        return
    run(["git", "clone", "--depth", "1", "--branch", ref, repo, str(source)], cwd=ROOT)


def configure_ffmpeg(source, build, samples):
    build.mkdir(parents=True, exist_ok=True)
    run(
        [
            str(source / "configure"),
            f"--samples={samples}",
            "--quiet",
            "--disable-doc",
            "--disable-programs",
            "--disable-x86asm",
        ],
        cwd=build,
    )


def check_samples(rmpeg_probe, output):
    require_tool("ffprobe")
    if not rmpeg_probe.exists():
        raise SystemExit(f"missing {rmpeg_probe}; build release binaries first")

    samples_dir = env_path("RMPEG_FFMPEG_SAMPLES_DIR", DEFAULT_SAMPLES)
    if not samples_dir.exists():
        raise SystemExit(f"missing {samples_dir}; run cargo xtask ffmpeg-samples-sync first")

    timeout = float(os.environ.get("RMPEG_FFMPEG_SAMPLE_TIMEOUT_SECONDS", "10"))
    sample_files = regular_files(samples_dir)
    limit = os.environ.get("RMPEG_FFMPEG_SAMPLE_LIMIT")
    if limit:
        sample_files = sample_files[: int(limit)]

    print(
        f"checking {len(sample_files)} FFmpeg FATE sample files with timeout {timeout:g}s",
        flush=True,
    )
    tests = []
    progress_every = int(os.environ.get("RMPEG_FFMPEG_PROGRESS_EVERY", "100"))
    last_progress = time.monotonic()
    for index, path in enumerate(sample_files, start=1):
        tests.append(check_one(path, samples_dir, rmpeg_probe, timeout))
        current_time = time.monotonic()
        if (
            index == len(sample_files)
            or (progress_every > 0 and index % progress_every == 0)
            or current_time - last_progress >= 15
        ):
            print(f"checked {index} / {len(sample_files)} sample files", flush=True)
            last_progress = current_time

    document = {
        "generated_at": now(),
        "rmpeg_commit": git_commit(),
        "ffmpeg_source": str(env_path("RMPEG_FFMPEG_SOURCE_DIR", DEFAULT_SOURCE)),
        "ffmpeg_commit": ffmpeg_commit(),
        "samples_dir": str(samples_dir),
        "sample_limit": int(limit) if limit else None,
        "summary": summarize(tests),
        "tests": tests,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    write_json(output, document)
    print(f"wrote {output}")
    print(json.dumps(document["summary"], sort_keys=True))
    return 1 if document["summary"]["errors"] else 0


def regular_files(samples_dir):
    return sorted(path for path in samples_dir.rglob("*") if path.is_file())


def check_one(sample, samples_dir, rmpeg_probe, timeout):
    relative = sample.relative_to(samples_dir).as_posix()
    ffprobe = run_capture(
        ["ffprobe", "-v", "error", "-show_format", "-show_streams", "-of", "json", str(sample)],
        timeout,
    )
    rmpeg = run_capture([str(rmpeg_probe), str(sample)], timeout)

    base = {
        "name": relative,
        "kind": "upstream-probe-json",
        "ffprobe_returncode": ffprobe.get("returncode"),
        "rmpeg_returncode": rmpeg.get("returncode"),
    }

    if ffprobe.get("timed_out") or rmpeg.get("timed_out"):
        return with_status(base, "error", "probe command timed out")
    if ffprobe.get("exception") or rmpeg.get("exception"):
        return with_status(base, "error", ffprobe.get("exception") or rmpeg.get("exception"))
    if "panicked at" in rmpeg.get("stderr", ""):
        return with_status(base, "error", f"rmpeg-probe panicked: {rmpeg.get('stderr', '')}")

    ffprobe_ok = ffprobe["returncode"] == 0
    rmpeg_ok = rmpeg["returncode"] == 0

    if not ffprobe_ok:
        if not rmpeg_ok:
            return with_status(base, "passed", "both ffprobe and rmpeg-probe rejected input")
        return with_status(base, "failed", "ffprobe rejected input but rmpeg-probe accepted it")

    if not rmpeg_ok:
        details = trim(rmpeg.get("stderr", ""))
        return with_status(
            base,
            "failed",
            f"ffprobe accepted input but rmpeg-probe rejected it: {details}",
        )

    try:
        expected = normalize_ffprobe_document(json.loads(ffprobe["stdout"]))
        actual = json.loads(rmpeg["stdout"])
        ok, details = compare_probe(expected, actual)
    except Exception as error:
        return with_status(base, "error", str(error))
    return with_status(base, "passed" if ok else "failed", details)


def run_capture(command, timeout):
    try:
        result = subprocess.run(
            command,
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return {"timed_out": True}
    except OSError as error:
        return {"exception": str(error)}
    return {
        "returncode": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
    }


def with_status(test, status, details):
    test["status"] = status
    test["details"] = trim(details)
    return test


def summarize(tests):
    ffprobe_accepted = sum(1 for test in tests if test.get("ffprobe_returncode") == 0)
    rmpeg_accepted = sum(1 for test in tests if test.get("rmpeg_returncode") == 0)
    return {
        "total": len(tests),
        "passed": sum(1 for test in tests if test["status"] == "passed"),
        "failed": sum(1 for test in tests if test["status"] == "failed"),
        "skipped": sum(1 for test in tests if test["status"] == "skipped"),
        "errors": sum(1 for test in tests if test["status"] == "error"),
        "ffprobe_accepted": ffprobe_accepted,
        "rmpeg_accepted": rmpeg_accepted,
        "rmpeg_rejected_ffprobe_accepted": sum(
            1
            for test in tests
            if test.get("ffprobe_returncode") == 0 and test.get("rmpeg_returncode") != 0
        ),
    }


def ffmpeg_commit():
    source = env_path("RMPEG_FFMPEG_SOURCE_DIR", DEFAULT_SOURCE)
    result = subprocess.run(
        ["git", "-C", str(source), "rev-parse", "--short", "HEAD"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    return result.stdout.strip() if result.returncode == 0 else "unknown"


def git_commit():
    result = subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    return result.stdout.strip() if result.returncode == 0 else "unknown"


def now():
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def env_path(name, fallback):
    return Path(os.environ.get(name, fallback)).expanduser().resolve()


def require_tool(name):
    if shutil.which(name) is None:
        raise SystemExit(f"required tool missing: {name}")


def run(command, cwd):
    print("running:", " ".join(str(part) for part in command))
    subprocess.run(command, cwd=cwd, check=True)


def write_json(path, document):
    path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")


def truthy(value):
    return value is not None and value.lower() not in {"", "0", "false", "no"}


def trim(text, limit=500):
    text = " ".join((text or "").split())
    return text[:limit]


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as error:
        print(f"command failed with exit {error.returncode}: {' '.join(error.cmd)}", file=sys.stderr)
        raise SystemExit(error.returncode)
