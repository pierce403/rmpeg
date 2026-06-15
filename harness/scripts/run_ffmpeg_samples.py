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
DEFAULT_FFPROBE_REF = "n8.0.1"
DEFAULT_FFPROBE_SOURCE = CACHE / "ffprobe-src"
DEFAULT_FFPROBE_BUILD = CACHE / "ffprobe-build"
REPORT = ROOT / "site" / "data" / "upstream-samples.json"


def main():
    parser = argparse.ArgumentParser(
        description="Use upstream FFmpeg FATE sample scripts and probe the downloaded corpus."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("sync")
    subparsers.add_parser("ffprobe-oracle")
    check_parser = subparsers.add_parser("check")
    check_parser.add_argument(
        "--rmpeg-probe", default=str(ROOT / "target" / "release" / "rmpeg-probe")
    )
    check_parser.add_argument("--rmpeg", default=str(ROOT / "target" / "release" / "rmpeg"))
    check_parser.add_argument("--ffprobe")
    check_parser.add_argument(
        "--skip-decode-execution",
        action="store_true",
        help="only run the probe comparison; omit rmpeg decode command execution inventory",
    )
    check_parser.add_argument("--output", default=str(REPORT))
    subparsers.add_parser("all")
    args = parser.parse_args()

    if args.command == "sync":
        sync_samples()
        return 0
    elif args.command == "ffprobe-oracle":
        build_ffprobe_oracle()
        return 0
    elif args.command == "check":
        return check_samples(
            Path(args.rmpeg_probe),
            Path(args.rmpeg),
            selected_ffprobe(args.ffprobe),
            Path(args.output),
            not args.skip_decode_execution,
        )
    sync_samples()
    return check_samples(
        ROOT / "target" / "release" / "rmpeg-probe",
        ROOT / "target" / "release" / "rmpeg",
        selected_ffprobe(None),
        REPORT,
        True,
    )


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
        fetch_ffmpeg_ref(source, ref)
        run(["git", "-C", str(source), "checkout", "--detach", "FETCH_HEAD"], cwd=ROOT)
        return
    run(["git", "clone", "--depth", "1", "--branch", ref, repo, str(source)], cwd=ROOT)


def fetch_ffmpeg_ref(source, ref):
    command = ["git", "-C", str(source), "fetch", "--depth", "1", "origin", ref]
    print("running:", " ".join(str(part) for part in command))
    result = subprocess.run(command, cwd=ROOT)
    if result.returncode == 0:
        return
    tag = ref.removeprefix("refs/tags/")
    run(["git", "-C", str(source), "fetch", "--depth", "1", "origin", "tag", tag], cwd=ROOT)


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


def build_ffprobe_oracle():
    require_tool("git")
    require_tool("make")
    require_tool("pkg-config")

    repo = os.environ.get("RMPEG_FFPROBE_REPO", DEFAULT_REPO)
    ref = os.environ.get("RMPEG_FFPROBE_REF", DEFAULT_FFPROBE_REF)
    source = env_path("RMPEG_FFPROBE_SOURCE_DIR", DEFAULT_FFPROBE_SOURCE)
    build = env_path("RMPEG_FFPROBE_BUILD_DIR", DEFAULT_FFPROBE_BUILD)
    samples = env_path("RMPEG_FFMPEG_SAMPLES_DIR", DEFAULT_SAMPLES)

    source.parent.mkdir(parents=True, exist_ok=True)
    samples.mkdir(parents=True, exist_ok=True)
    checkout_ffmpeg(repo, ref, source)
    configure_ffprobe(source, build, samples)

    jobs = os.environ.get("RMPEG_FFPROBE_MAKE_JOBS", str(os.cpu_count() or 2))
    run(["make", "-C", str(build), f"-j{jobs}", "ffprobe"], cwd=ROOT)
    ffprobe = build / "ffprobe"
    if not ffprobe.exists():
        raise SystemExit(f"expected ffprobe at {ffprobe}")
    print(f"built ffprobe oracle at {ffprobe}")
    print(ffprobe_version(str(ffprobe)))


def configure_ffprobe(source, build, samples):
    build.mkdir(parents=True, exist_ok=True)
    run(
        [
            str(source / "configure"),
            f"--samples={samples}",
            "--quiet",
            "--disable-doc",
            "--disable-ffmpeg",
            "--disable-ffplay",
            "--disable-x86asm",
            "--enable-libxml2",
        ],
        cwd=build,
    )


def selected_ffprobe(argument):
    return argument or os.environ.get("RMPEG_FFPROBE", "ffprobe")


def check_samples(rmpeg_probe, rmpeg, ffprobe_executable, output, decode_execution):
    require_tool(ffprobe_executable)
    if not rmpeg_probe.exists():
        raise SystemExit(f"missing {rmpeg_probe}; build release binaries first")
    if decode_execution and not rmpeg.exists():
        raise SystemExit(f"missing {rmpeg}; build release binaries first")

    samples_dir = env_path("RMPEG_FFMPEG_SAMPLES_DIR", DEFAULT_SAMPLES)
    if not samples_dir.exists():
        raise SystemExit(f"missing {samples_dir}; run cargo xtask ffmpeg-samples-sync first")

    timeout = float(os.environ.get("RMPEG_FFMPEG_SAMPLE_TIMEOUT_SECONDS", "10"))
    decode_timeout = float(os.environ.get("RMPEG_FFMPEG_DECODE_TIMEOUT_SECONDS", str(timeout)))
    sample_files = regular_files(samples_dir)
    limit = os.environ.get("RMPEG_FFMPEG_SAMPLE_LIMIT")
    if limit:
        sample_files = sample_files[: int(limit)]

    print(
        f"checking {len(sample_files)} FFmpeg FATE sample files with {ffprobe_executable} "
        f"and timeout {timeout:g}s",
        flush=True,
    )
    tests = []
    decode_tests = []
    progress_every = int(os.environ.get("RMPEG_FFMPEG_PROGRESS_EVERY", "100"))
    last_progress = time.monotonic()
    for index, path in enumerate(sample_files, start=1):
        probe_test, decode_test = check_one(
            path,
            samples_dir,
            rmpeg_probe,
            rmpeg,
            ffprobe_executable,
            timeout,
            decode_timeout,
            decode_execution,
        )
        tests.append(probe_test)
        if decode_test is not None:
            decode_tests.append(decode_test)
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
        "ffprobe_executable": ffprobe_executable,
        "ffprobe_version": ffprobe_version(ffprobe_executable),
        "samples_dir": str(samples_dir),
        "sample_limit": int(limit) if limit else None,
        "summary": summarize(tests),
        "tests": tests,
    }
    if decode_execution:
        document["decode_execution_summary"] = summarize_decode_execution(decode_tests)
        document["decode_execution_tests"] = decode_tests
    output.parent.mkdir(parents=True, exist_ok=True)
    write_json(output, document)
    print(f"wrote {output}")
    print(json.dumps(document["summary"], sort_keys=True))
    if decode_execution:
        print(json.dumps(document["decode_execution_summary"], sort_keys=True))
    return 1 if document["summary"]["errors"] else 0


def regular_files(samples_dir):
    return sorted(path for path in samples_dir.rglob("*") if path.is_file())


def check_one(
    sample,
    samples_dir,
    rmpeg_probe,
    rmpeg,
    ffprobe_executable,
    timeout,
    decode_timeout,
    decode_execution,
):
    relative = sample.relative_to(samples_dir).as_posix()
    ffprobe = run_capture(
        [
            ffprobe_executable,
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
            str(sample),
        ],
        timeout,
    )
    rmpeg_probe_result = run_capture([str(rmpeg_probe), str(sample)], timeout)

    base = {
        "name": relative,
        "kind": "upstream-probe-json",
        "ffprobe_returncode": ffprobe.get("returncode"),
        "rmpeg_returncode": rmpeg_probe_result.get("returncode"),
    }
    decode_test = None

    if ffprobe.get("timed_out") or rmpeg_probe_result.get("timed_out"):
        return with_status(base, "error", "probe command timed out"), decode_test
    if ffprobe.get("exception") or rmpeg_probe_result.get("exception"):
        return (
            with_status(
                base, "error", ffprobe.get("exception") or rmpeg_probe_result.get("exception")
            ),
            decode_test,
        )
    if "panicked at" in rmpeg_probe_result.get("stderr", ""):
        return (
            with_status(
                base,
                "error",
                f"rmpeg-probe panicked: {rmpeg_probe_result.get('stderr', '')}",
            ),
            decode_test,
        )

    ffprobe_ok = ffprobe["returncode"] == 0
    rmpeg_ok = rmpeg_probe_result["returncode"] == 0

    if not ffprobe_ok:
        if decode_execution:
            if rmpeg_ok:
                decode_test = clean_decode_failure(
                    relative,
                    "ffprobe rejected input but rmpeg-probe accepted it",
                    command_surface="ffprobe-rejected",
                )
            else:
                decode_test = clean_decode_success(
                    relative,
                    "both ffprobe and rmpeg-probe rejected input; no decode surface expected",
                    command_surface="ffprobe-rejected",
                )
        if not rmpeg_ok:
            return (
                with_status(base, "passed", "both ffprobe and rmpeg-probe rejected input"),
                decode_test,
            )
        return (
            with_status(base, "failed", "ffprobe rejected input but rmpeg-probe accepted it"),
            decode_test,
        )

    if not rmpeg_ok:
        details = trim(rmpeg_probe_result.get("stderr", ""))
        return (
            with_status(
                base,
                "failed",
                f"ffprobe accepted input but rmpeg-probe rejected it: {details}",
            ),
            decode_test,
        )

    decode_probe = None
    try:
        expected = normalize_ffprobe_document(json.loads(ffprobe["stdout"]))
        actual = json.loads(rmpeg_probe_result["stdout"])
        decode_probe = expected
        ok, details = compare_probe(expected, actual)
    except Exception as error:
        return with_status(base, "error", str(error)), decode_test

    if decode_execution:
        decode_test = check_decode_execution(relative, sample, rmpeg, decode_probe, decode_timeout)
    return with_status(base, "passed" if ok else "failed", details), decode_test


def check_decode_execution(relative, sample, rmpeg, probe, timeout):
    commands = decode_commands(rmpeg, sample, probe)
    if not commands:
        return clean_decode_failure(
            relative,
            "ffprobe accepted input but rmpeg has no decode command surface for its streams",
            command_surface="no-audio-video-stream",
        )
    failures = []
    last_test = None
    for command, surface in commands:
        result = run_execution_capture(command, timeout)
        test = {
            "name": relative,
            "kind": "upstream-decode-execution",
            "command_surface": surface,
            "rmpeg_returncode": result.get("returncode"),
        }
        last_test = test
        if result.get("timed_out"):
            return with_status(test, "error", "decode command timed out")
        if result.get("exception"):
            return with_status(test, "error", result.get("exception"))
        if "panicked at" in result.get("stderr", ""):
            return with_status(test, "error", f"rmpeg decode panicked: {result.get('stderr', '')}")
        if result.get("returncode") == 0:
            return with_status(test, "passed", "rmpeg decode command completed")
        failures.append(f"{surface}: {trim(result.get('stderr', ''))}")
    return with_status(last_test, "failed", " | ".join(failures))


def decode_commands(rmpeg, sample, probe):
    streams = probe.get("streams", [])
    commands = []
    for stream in streams:
        if stream.get("codec_type") == "audio" and audio_stream_has_shape(stream):
            commands.append(([str(rmpeg), "decode", str(sample), "--framemd5"], "audio"))
    for stream in streams:
        if stream.get("codec_type") == "video":
            if stream.get("codec_name") in {"alias_pix", "bmp", "brender_pix", "dds", "dpx", "fits", "pbm", "pgm", "png", "ppm", "ptx", "sgi", "sunrast", "targa", "xbm"} or sample.suffix.lower() in {
                ".bmp",
                ".dds",
                ".dpx",
                ".fit",
                ".fits",
                ".fts",
                ".pbm",
                ".pgm",
                ".pnm",
                ".png",
                ".ppm",
                ".ptx",
                ".ras",
                ".sgi",
                ".sun",
                ".tga",
                ".xbm",
            }:
                commands.append(([str(rmpeg), "decode-image", str(sample), "--framemd5"], "image"))
            else:
                commands.append(([str(rmpeg), "decode-video", str(sample), "--framemd5"], "video"))
    for stream in streams:
        if stream.get("codec_type") == "audio" and not audio_stream_has_shape(stream):
            commands.append(([str(rmpeg), "decode", str(sample), "--framemd5"], "audio"))
    if probe.get("format"):
        commands.append(([str(rmpeg), "demux", str(sample), "--null"], "stream-copy"))
    return commands


def audio_stream_has_shape(stream):
    return (stream.get("sample_rate") or 0) > 0 and (stream.get("channels") or 0) > 0


def clean_decode_failure(relative, details, command_surface):
    return with_status(
        {
            "name": relative,
            "kind": "upstream-decode-execution",
            "command_surface": command_surface,
            "rmpeg_returncode": None,
        },
        "failed",
        details,
    )


def clean_decode_success(relative, details, command_surface):
    return with_status(
        {
            "name": relative,
            "kind": "upstream-decode-execution",
            "command_surface": command_surface,
            "rmpeg_returncode": None,
        },
        "passed",
        details,
    )


def run_execution_capture(command, timeout):
    try:
        result = subprocess.run(
            command,
            cwd=ROOT,
            text=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return {"timed_out": True}
    except OSError as error:
        return {"exception": str(error)}
    return {
        "returncode": result.returncode,
        "stderr": result.stderr,
    }


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


def summarize_decode_execution(tests):
    total = len(tests)
    passed = sum(1 for test in tests if test["status"] == "passed")
    failed = sum(1 for test in tests if test["status"] == "failed")
    errors = sum(1 for test in tests if test["status"] == "error")
    attempted = sum(1 for test in tests if test.get("rmpeg_returncode") is not None)
    return {
        "total": total,
        "attempted": attempted,
        "clean": passed + failed,
        "passed": passed,
        "failed": failed,
        "errors": errors,
        "skipped": sum(1 for test in tests if test["status"] == "skipped"),
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


def ffprobe_version(ffprobe_executable):
    result = subprocess.run(
        [ffprobe_executable, "-version"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    if result.returncode != 0:
        return "unknown"
    return result.stdout.splitlines()[0] if result.stdout else "unknown"


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
