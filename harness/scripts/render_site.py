#!/usr/bin/env python3
import html
import json
import shutil
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TEMPLATE = ROOT / "site" / "templates" / "index.html"
DIST = ROOT / "site" / "dist"
STATIC = ROOT / "site" / "static"
CORRECTNESS = ROOT / "site" / "data" / "correctness.json"
BENCH_SUMMARY = ROOT / "site" / "data" / "benchmark-summary.json"
UPSTREAM_SAMPLES = ROOT / "site" / "data" / "upstream-samples.json"
RUNS = ROOT / "agents" / "runs"


def main():
    correctness = read_json(CORRECTNESS, empty_correctness())
    benchmark_summary = read_json(BENCH_SUMMARY, empty_benchmarks())
    upstream_samples = read_json(UPSTREAM_SAMPLES, empty_upstream_samples())
    phase_stats = phase_run_stats(correctness, upstream_samples)
    rendered = TEMPLATE.read_text()
    replacements = {
        "generated_at": escape(correctness.get("generated_at", "unknown")),
        "og_description": escape(phase_description(phase_stats)),
        "current_status": render_current_status(correctness),
        "phase_progress": render_phase_progress(phase_stats),
        "correctness": render_correctness(correctness),
        "upstream_samples": render_upstream_samples(upstream_samples),
        "benchmarks": render_benchmarks(benchmark_summary),
        "known_failures": render_known_failures(correctness),
        "autoresearch_log": render_autoresearch_log(),
    }
    for key, value in replacements.items():
        rendered = rendered.replace("{{" + key + "}}", value)
    DIST.mkdir(parents=True, exist_ok=True)
    (DIST / "index.html").write_text(rendered)
    copy_static_files()
    write_og_card(DIST / "og-card.png", phase_stats)
    print(f"wrote {DIST / 'index.html'}")


def read_json(path, fallback):
    if path.exists():
        return json.loads(path.read_text())
    return fallback


def empty_correctness():
    return {
        "generated_at": "unknown",
        "rmpeg_commit": "unknown",
        "ffmpeg_version": "unknown",
        "summary": {"total": 0, "passed": 0, "failed": 0, "skipped": 0},
        "tests": [],
    }


def empty_benchmarks():
    return {"generated_at": "unknown", "benchmarks": []}


def empty_upstream_samples():
    return {
        "generated_at": "unknown",
        "ffmpeg_commit": "unknown",
        "samples_dir": "unknown",
        "summary": {},
        "tests": [],
    }


def render_current_status(correctness):
    tests = correctness.get("tests", [])
    rows = [
        status_row("WAV/PCM metadata", tests, "probe-json", "probe wav"),
        status_row("WAV/PCM decode/hash", tests, "framemd5", "decode/hash wav"),
        status_row("MP3 metadata", tests, "probe-json", "probe mp3"),
        status_row("MP3 decode/hash", tests, "framemd5", "decode/hash mp3"),
        status_row("FLAC metadata", tests, "probe-json", "probe flac"),
        status_row("FLAC decode/hash", tests, "framemd5", "decode/hash flac"),
        status_row("Ogg audio metadata", tests, "probe-json", "probe ogg"),
        status_row("Ogg Vorbis decode/hash", tests, "framemd5", "decode/hash ogg tone_vorbis"),
        status_row("Ogg Opus decode/hash", tests, "framemd5", "decode/hash ogg tone_opus"),
        status_row("MP4/MOV metadata", tests, "probe-json", "probe mp4"),
        status_row("H.264/AAC metadata", tests, "probe-json", "probe mp4"),
        status_row("H.264/AAC decode/hash", tests, "framemd5", "decode/hash mp4"),
        status_row("Video decode/hash", tests, "framemd5", "decode/video"),
        status_row("Image decode/hash", tests, "framemd5", "decode/image"),
        status_row("Audio filters", tests, "filter", "filter audio"),
        status_row("Seeking", tests, "seek", "seek audio"),
        status_row("Resampling", tests, "resample", "resample audio"),
        status_row("Remuxing", tests, "remux", "remux"),
    ]
    return table(["Area", "Status"], rows)


def render_phase_progress(stats):
    total = stats["total"]
    runnable = stats["runnable"]
    passed = stats["passed"]
    failed = stats["failed"]
    errors = stats["errors"]
    skipped = stats["skipped"]
    percent = stats["percent"]
    if total == 0:
        return (
            '<section class="phase-panel" aria-labelledby="phase-1-title">'
            '<div class="phase-row"><div>'
            '<p class="phase-kicker">Phase 1</p>'
            f'<h2 id="phase-1-title">{escape(stats["headline"])}</h2>'
            f"<p>{escape(stats['no_data'])}</p>"
            "</div></div></section>"
        )

    comparison_sentence = (
        f"All {passed} rows pass the current FFmpeg compatibility check."
        if failed == 0 and errors == 0 and skipped == 0
        else f"Failed comparisons stay red but count as runnable; {passed} rows currently pass."
    )
    attempted = stats.get("attempted")
    attempted_metric = (
        f"<span><strong>{attempted}</strong> commands attempted</span>"
        if attempted is not None
        else ""
    )
    return f"""
      <section class="phase-panel" aria-labelledby="phase-1-title">
        <div class="phase-row">
          <div>
            <p class="phase-kicker">Phase 1</p>
            <h2 id="phase-1-title">{escape(stats["headline"])}</h2>
            <p>rmpeg currently gives {runnable} of {total} {escape(stats["scope"])} a clean execution row. {comparison_sentence}</p>
          </div>
          <div class="phase-percent">{percent:.1f}%</div>
        </div>
        <div class="phase-progress-track" role="img" aria-label="Phase 1 progress {percent:.1f} percent">
          <div class="phase-progress-fill" style="width: {percent:.1f}%"></div>
        </div>
        <div class="phase-metrics">
          <span><strong>{runnable}</strong> runnable</span>
          <span><strong>{passed}</strong> passed rows</span>
          <span><strong>{failed}</strong> failed comparisons</span>
          <span><strong>{errors}</strong> errors</span>
          <span><strong>{skipped}</strong> skipped</span>
          {attempted_metric}
          <span><strong>{total}</strong> {escape(stats["total_label"])}</span>
        </div>
      </section>
    """


def phase_description(stats):
    if stats["total"] == 0:
        return "Phase 1: sample execution progress has not been measured yet."
    return (
        f"Phase 1: rmpeg currently gives {stats['runnable']} of "
        f"{stats['total']} {stats['scope']} a clean execution row "
        f"({stats['percent']:.1f}% execution coverage); "
        f"{stats['passed']} pass the current FFmpeg compatibility check."
    )


def phase_run_stats(correctness, upstream_samples):
    decode_summary = upstream_samples.get("decode_execution_summary") or {}
    upstream_total = int(decode_summary.get("total", 0) or 0)
    if upstream_total:
        clean = int(decode_summary.get("clean", 0) or 0)
        return {
            "source": "upstream",
            "headline": "Upstream Sample Execution Progress",
            "scope": "synced upstream sample files",
            "total_label": "upstream samples",
            "no_data": "No upstream sample execution report has been generated yet.",
            "total": upstream_total,
            "runnable": clean,
            "passed": int(decode_summary.get("passed", 0) or 0),
            "failed": int(decode_summary.get("failed", 0) or 0),
            "errors": int(decode_summary.get("errors", 0) or 0),
            "skipped": int(decode_summary.get("skipped", 0) or 0),
            "attempted": int(decode_summary.get("attempted", 0) or 0),
            "percent": clean / upstream_total * 100,
        }
    stats = clean_run_stats(correctness)
    stats.update(
        {
            "source": "mirrored",
            "headline": "Mirrored Sample Execution Progress",
            "scope": "mirrored sample checks",
            "total_label": "mirrored checks",
            "no_data": "No mirrored correctness report has been generated yet.",
            "attempted": None,
        }
    )
    return stats


def clean_run_stats(correctness):
    summary = correctness.get("summary", {})
    tests = correctness.get("tests", [])
    total = int(summary.get("total", len(tests)) or 0)
    passed = int(summary.get("passed", 0) or 0)
    failed = int(summary.get("failed", 0) or 0)
    errors = int(summary.get("errors", 0) or 0)
    skipped = int(summary.get("skipped", 0) or 0)
    runnable = passed + failed
    percent = runnable / total * 100 if total else 0.0
    return {
        "total": total,
        "runnable": runnable,
        "passed": passed,
        "failed": failed,
        "errors": errors,
        "skipped": skipped,
        "percent": percent,
    }


def status_row(label, tests, kind, prefix):
    selected = [
        test
        for test in tests
        if test.get("kind") == kind and test.get("name", "").startswith(prefix)
    ]
    return (label, ratio(selected), status_class(selected))


def ratio(tests):
    if not tests:
        return "0/0 passing"
    passed = sum(1 for test in tests if test.get("status") == "passed")
    return f"{passed}/{len(tests)} passing"


def status_class(tests):
    if not tests:
        return "failed"
    if any(test.get("status") in {"failed", "error"} for test in tests):
        return "failed"
    if all(test.get("status") == "passed" for test in tests):
        return "passed"
    return "failed"


def render_correctness(correctness):
    rows = []
    for test in correctness.get("tests", []):
        rows.append(
            (
                escape(test.get("name", "")),
                escape(test.get("kind", "")),
                badge(test.get("status", "unknown")),
                escape(test.get("details", "")),
            )
        )
    if not rows:
        rows.append(("No mirrored tests have been run.", "", badge("error"), ""))
    return table(["Test", "Kind", "Status", "Details"], rows)


def render_benchmarks(summary):
    rows = []
    for bench in summary.get("benchmarks", []):
        rows.append(
            (
                escape(bench.get("name", "")),
                seconds(bench.get("ffmpeg_seconds_mean", 0.0)),
                seconds(bench.get("rmpeg_seconds_mean", 0.0)),
                escape(bench.get("relative", "")),
                badge(bench.get("status", "unknown")),
            )
        )
    if not rows:
        rows.append(("No benchmarks have been run.", "", "", "not measured", badge("skipped")))
    return table(["Benchmark", "FFmpeg mean", "rmpeg mean", "Relative", "Status"], rows)


def render_upstream_samples(report):
    summary = report.get("summary", {})
    decode_summary = report.get("decode_execution_summary")
    total = int(summary.get("total", 0) or 0)
    if total == 0:
        return (
            "<p>Not run yet. Run <code>cargo xtask ffmpeg-samples</code> to sync the upstream "
            "FFmpeg FATE corpus with <code>make fate-rsync</code> and probe every regular file.</p>"
        )
    rows = [
        ("Generated", escape(report.get("generated_at", "unknown"))),
        ("FFmpeg commit", escape(report.get("ffmpeg_commit", "unknown"))),
        ("ffprobe version", escape(report.get("ffprobe_version", "unknown"))),
        ("Samples directory", f"<code>{escape(report.get('samples_dir', 'unknown'))}</code>"),
        ("Total files checked", str(total)),
        ("ffprobe accepted", str(summary.get("ffprobe_accepted", 0))),
        ("rmpeg-probe accepted", str(summary.get("rmpeg_accepted", 0))),
        ("Passed", str(summary.get("passed", 0))),
        ("Failed", str(summary.get("failed", 0))),
        ("Errors", str(summary.get("errors", 0))),
    ]
    if decode_summary:
        decode_tests = report.get("decode_execution_tests", [])
        command_successes = sum(
            1
            for test in decode_tests
            if test.get("status") == "passed" and test.get("rmpeg_returncode") == 0
        )
        compatible_rejections = sum(
            1
            for test in decode_tests
            if test.get("status") == "passed" and test.get("rmpeg_returncode") is None
        )
        rows.extend(
            [
                ("Decode execution clean rows", str(decode_summary.get("clean", 0))),
                ("Decode execution passed rows", str(decode_summary.get("passed", 0))),
                ("Decode execution total rows", str(decode_summary.get("total", 0))),
                ("Decode commands attempted", str(decode_summary.get("attempted", 0))),
                ("Decode commands passed", str(command_successes)),
                ("Compatible no-command rows", str(compatible_rejections)),
                ("Decode clean failures", str(decode_summary.get("failed", 0))),
                ("Decode execution errors", str(decode_summary.get("errors", 0))),
            ]
        )
    failures = [
        test for test in report.get("tests", []) if test.get("status") in {"failed", "error"}
    ][:25]
    decode_errors = [
        test
        for test in report.get("decode_execution_tests", [])
        if test.get("status") == "error"
    ][:25]
    failure_rows = [
        (
            escape(test.get("name", "")),
            badge(test.get("status", "unknown")),
            escape(test.get("details", "")),
        )
        for test in failures
    ]
    if not failure_rows:
        failure_rows.append(("No failures in the upstream corpus probe run.", badge("passed"), ""))
    rendered = table(["Metric", "Value"], rows) + table(["Sample", "Status", "Details"], failure_rows)
    if decode_summary:
        decode_rows = [
            (
                escape(test.get("name", "")),
                escape(test.get("command_surface", "")),
                badge(test.get("status", "unknown")),
                escape(test.get("details", "")),
            )
            for test in decode_errors
        ]
        if not decode_rows:
            decode_rows.append(
                (
                    "No decode execution errors in the upstream corpus run.",
                    "",
                    badge("passed"),
                    "",
                )
            )
        rendered += table(["Sample", "Surface", "Status", "Details"], decode_rows)
    return rendered


def render_known_failures(correctness):
    failures = [
        test
        for test in correctness.get("tests", [])
        if test.get("status") in {"failed", "error", "skipped"}
    ]
    if not failures:
        return "<p>No mirrored failures in the current slice. Every mirrored check is running cleanly.</p>"
    items = []
    for test in failures:
        items.append(
            f"<li><strong>{escape(test.get('name', ''))}</strong>: "
            f"{badge(test.get('status', 'unknown'))} {escape(test.get('details', ''))}</li>"
        )
    return "<ul>" + "\n".join(items) + "</ul>"


def render_autoresearch_log():
    logs = sorted(RUNS.glob("*.md"), reverse=True)[:5] if RUNS.exists() else []
    if not logs:
        return "<p>No autoresearch runs recorded yet.</p>"
    items = []
    for path in logs:
        first_line = path.read_text().splitlines()[0] if path.read_text().splitlines() else path.name
        items.append(f"<li><code>{escape(path.name)}</code> {escape(first_line)}</li>")
    return "<ul>" + "\n".join(items) + "</ul>"


def table(headers, rows):
    head = "".join(f"<th>{escape(header)}</th>" for header in headers)
    body_rows = []
    for row in rows:
        row_class = ""
        if row and isinstance(row[-1], str) and row[-1] in {"passed", "failed"}:
            row_class = f' class="{row[-1]}"'
            row = row[:-1]
        cells = "".join(f"<td>{cell}</td>" for cell in row)
        body_rows.append(f"<tr{row_class}>{cells}</tr>")
    return f"<table><thead><tr>{head}</tr></thead><tbody>{''.join(body_rows)}</tbody></table>"


def badge(status):
    safe = escape(status)
    css = "failed" if status in {"failed", "error"} else status
    return f'<span class="badge {escape(css)}">{safe}</span>'


def seconds(value):
    try:
        return f"{float(value):.6f}s"
    except (TypeError, ValueError):
        return "unknown"


def escape(value):
    return html.escape(str(value), quote=True)


def write_og_card(path, stats):
    width = 1200
    height = 630
    background = (247, 247, 242)
    ink = (23, 27, 31)
    muted = (89, 96, 101)
    green = (47, 111, 101)
    track = (226, 227, 219)
    border = (199, 203, 194)
    pixels = bytearray(background * (width * height))

    total = stats["total"]
    runnable = stats["runnable"]
    percent = stats["percent"]

    draw_text(pixels, width, height, 80, 72, "RMPEG", 13, ink)
    draw_text(pixels, width, height, 84, 178, "PHASE 1", 6, green)
    draw_text(pixels, width, height, 80, 246, "SAMPLE EXECUTION", 7, ink)
    if total:
        draw_text(pixels, width, height, 80, 338, f"{percent:.1f}%", 13, green)
        noun = "SAMPLES" if stats.get("source") == "upstream" else "CHECKS"
        metric = f"{runnable} / {total} {noun} CLEAN"
    else:
        draw_text(pixels, width, height, 80, 338, "NO DATA", 13, green)
        metric = "RUN CARGO XTASK SITE"

    bar_x = 80
    bar_y = 478
    bar_width = 1040
    bar_height = 56
    fill_rect(pixels, width, height, bar_x, bar_y, bar_width, bar_height, border)
    fill_rect(
        pixels,
        width,
        height,
        bar_x + 4,
        bar_y + 4,
        bar_width - 8,
        bar_height - 8,
        track,
    )
    fill_width = round((bar_width - 8) * percent / 100)
    if fill_width > 0:
        fill_rect(pixels, width, height, bar_x + 4, bar_y + 4, fill_width, bar_height - 8, green)

    draw_text(pixels, width, height, 82, 558, metric, 5, muted)
    write_png(path, width, height, pixels)


FONT = {
    " ": ["00000", "00000", "00000", "00000", "00000", "00000", "00000"],
    "%": ["11001", "11010", "00100", "01000", "10110", "00110", "00000"],
    ".": ["00000", "00000", "00000", "00000", "00000", "01100", "01100"],
    "/": ["00001", "00010", "00100", "01000", "10000", "00000", "00000"],
    "0": ["01110", "10001", "10011", "10101", "11001", "10001", "01110"],
    "1": ["00100", "01100", "00100", "00100", "00100", "00100", "01110"],
    "2": ["01110", "10001", "00001", "00010", "00100", "01000", "11111"],
    "3": ["11110", "00001", "00001", "01110", "00001", "00001", "11110"],
    "4": ["00010", "00110", "01010", "10010", "11111", "00010", "00010"],
    "5": ["11111", "10000", "11110", "00001", "00001", "10001", "01110"],
    "6": ["00110", "01000", "10000", "11110", "10001", "10001", "01110"],
    "7": ["11111", "00001", "00010", "00100", "01000", "01000", "01000"],
    "8": ["01110", "10001", "10001", "01110", "10001", "10001", "01110"],
    "9": ["01110", "10001", "10001", "01111", "00001", "00010", "01100"],
    "A": ["01110", "10001", "10001", "11111", "10001", "10001", "10001"],
    "C": ["01111", "10000", "10000", "10000", "10000", "10000", "01111"],
    "D": ["11110", "10001", "10001", "10001", "10001", "10001", "11110"],
    "E": ["11111", "10000", "10000", "11110", "10000", "10000", "11111"],
    "F": ["11111", "10000", "10000", "11110", "10000", "10000", "10000"],
    "G": ["01111", "10000", "10000", "10111", "10001", "10001", "01111"],
    "H": ["10001", "10001", "10001", "11111", "10001", "10001", "10001"],
    "I": ["11111", "00100", "00100", "00100", "00100", "00100", "11111"],
    "K": ["10001", "10010", "10100", "11000", "10100", "10010", "10001"],
    "L": ["10000", "10000", "10000", "10000", "10000", "10000", "11111"],
    "M": ["10001", "11011", "10101", "10101", "10001", "10001", "10001"],
    "N": ["10001", "11001", "10101", "10011", "10001", "10001", "10001"],
    "O": ["01110", "10001", "10001", "10001", "10001", "10001", "01110"],
    "P": ["11110", "10001", "10001", "11110", "10000", "10000", "10000"],
    "R": ["11110", "10001", "10001", "11110", "10100", "10010", "10001"],
    "S": ["01111", "10000", "10000", "01110", "00001", "00001", "11110"],
    "T": ["11111", "00100", "00100", "00100", "00100", "00100", "00100"],
    "U": ["10001", "10001", "10001", "10001", "10001", "10001", "01110"],
    "X": ["10001", "10001", "01010", "00100", "01010", "10001", "10001"],
    "Y": ["10001", "10001", "01010", "00100", "00100", "00100", "00100"],
}


def draw_text(pixels, width, height, x, y, text, scale, color):
    cursor = x
    for char in text.upper():
        glyph = FONT.get(char, FONT[" "])
        for row, bits in enumerate(glyph):
            for col, bit in enumerate(bits):
                if bit == "1":
                    fill_rect(
                        pixels,
                        width,
                        height,
                        cursor + col * scale,
                        y + row * scale,
                        scale,
                        scale,
                        color,
                    )
        cursor += 6 * scale


def fill_rect(pixels, width, height, x, y, rect_width, rect_height, color):
    x0 = max(0, int(x))
    y0 = max(0, int(y))
    x1 = min(width, int(x + rect_width))
    y1 = min(height, int(y + rect_height))
    if x0 >= x1 or y0 >= y1:
        return
    red, green, blue = color
    for row in range(y0, y1):
        offset = (row * width + x0) * 3
        for _ in range(x0, x1):
            pixels[offset] = red
            pixels[offset + 1] = green
            pixels[offset + 2] = blue
            offset += 3


def write_png(path, width, height, pixels):
    stride = width * 3
    raw = bytearray()
    for row in range(height):
        raw.append(0)
        start = row * stride
        raw.extend(pixels[start : start + stride])
    payload = (
        png_chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + png_chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + png_chunk(b"IEND", b"")
    )
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + payload)


def png_chunk(kind, data):
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    )


def copy_static_files():
    if not STATIC.exists():
        return
    for path in STATIC.iterdir():
        if path.is_file() and path.name != ".gitkeep":
            shutil.copy2(path, DIST / path.name)


if __name__ == "__main__":
    main()
