#!/usr/bin/env python3
import math
import shutil
import struct
import subprocess
import wave
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SAMPLES = ROOT / "harness" / "samples"


def clamp_i16(value):
    return max(-32768, min(32767, int(value)))


def write_pcm16(path, sample_rate, channels, frames, sample_fn):
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = bytearray()
    for i in range(frames):
        for ch in range(channels):
            payload.extend(struct.pack("<h", clamp_i16(sample_fn(i, ch))))
    with wave.open(str(path), "wb") as handle:
        handle.setnchannels(channels)
        handle.setsampwidth(2)
        handle.setframerate(sample_rate)
        handle.writeframes(bytes(payload))


def write_pcm_u8(path, sample_rate, channels, frames):
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = bytearray()
    for i in range(frames):
        value = 128 + int(80 * math.sin(2 * math.pi * 220 * i / sample_rate))
        for _ in range(channels):
            payload.append(max(0, min(255, value)))
    with wave.open(str(path), "wb") as handle:
        handle.setnchannels(channels)
        handle.setsampwidth(1)
        handle.setframerate(sample_rate)
        handle.writeframes(bytes(payload))


def chunk(tag, payload):
    out = bytearray()
    out.extend(tag)
    out.extend(struct.pack("<I", len(payload)))
    out.extend(payload)
    if len(payload) % 2:
        out.append(0)
    return bytes(out)


def write_odd_chunks(path):
    frames = 64
    sample_rate = 8000
    channels = 1
    pcm = b"".join(struct.pack("<h", i * 23 - 512) for i in range(frames))
    fmt = struct.pack("<HHIIHH", 1, channels, sample_rate, sample_rate * 2, 2, 16)
    body = bytearray()
    body.extend(b"WAVE")
    body.extend(chunk(b"JUNK", b"odd!!"))
    body.extend(chunk(b"fmt ", fmt))
    body.extend(chunk(b"data", pcm))
    path.write_bytes(b"RIFF" + struct.pack("<I", len(body)) + body)


def main():
    SAMPLES.mkdir(parents=True, exist_ok=True)
    clean_generated_samples()
    write_pcm16(SAMPLES / "mono_silence_8k.wav", 8000, 1, 8000, lambda _i, _ch: 0)
    write_pcm16(
        SAMPLES / "stereo_44k_sine.wav",
        44100,
        2,
        44100,
        lambda i, ch: math.sin(2 * math.pi * (220 + ch * 220) * i / 44100) * 12000,
    )
    write_pcm16(
        SAMPLES / "short_100ms.wav",
        8000,
        1,
        800,
        lambda i, _ch: ((i % 64) - 32) * 128,
    )
    write_pcm16(
        SAMPLES / "tiny.wav",
        8000,
        1,
        800,
        lambda i, _ch: math.sin(2 * math.pi * 440 * i / 8000) * 9000,
    )
    write_odd_chunks(SAMPLES / "odd_chunks.wav")
    write_pcm_u8(SAMPLES / "pcm_u8.wav", 8000, 1, 800)
    (SAMPLES / "truncated_riff.wav").write_bytes(b"RIFF\x08\x00\x00\x00WAVEfmt ")
    write_png_sample(SAMPLES / "tiny_rgb.png")
    write_compressed_samples()
    print(f"generated samples in {SAMPLES}")


def clean_generated_samples():
    for pattern in ("*.wav", "*.mp3", "*.mp4", "*.flac", "*.ogg", "*.png"):
        for path in SAMPLES.glob(pattern):
            path.unlink()


def write_png_sample(path):
    width = 2
    height = 2
    rows = [
        bytes([0, 255, 0, 0, 0, 255, 0]),
        bytes([0, 0, 0, 255, 255, 255, 255]),
    ]
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    payload = (
        png_chunk(b"IHDR", ihdr)
        + png_chunk(b"IDAT", zlib.compress(b"".join(rows), 9))
        + png_chunk(b"IEND", b"")
    )
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + payload)


def png_chunk(tag, payload):
    return (
        struct.pack(">I", len(payload))
        + tag
        + payload
        + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF)
    )


def write_compressed_samples():
    if shutil.which("ffmpeg") is None:
        raise SystemExit("required tool missing for compressed samples: ffmpeg")
    run(
        [
            "ffmpeg",
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100:duration=1",
            "-ac",
            "2",
            "-codec:a",
            "libmp3lame",
            "-b:a",
            "96k",
            "-write_xing",
            "0",
            str(SAMPLES / "tone_mp3.mp3"),
        ]
    )
    run(
        [
            "ffmpeg",
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=330:sample_rate=44100:duration=1",
            "-ac",
            "2",
            "-c:a",
            "flac",
            str(SAMPLES / "tone_flac.flac"),
        ]
    )
    run(
        [
            "ffmpeg",
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=550:sample_rate=48000:duration=1",
            "-ac",
            "1",
            "-c:a",
            "libopus",
            "-b:a",
            "48k",
            str(SAMPLES / "tone_opus.ogg"),
        ]
    )
    run(
        [
            "ffmpeg",
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=770:sample_rate=44100:duration=1",
            "-ac",
            "2",
            "-c:a",
            "libvorbis",
            "-b:a",
            "96k",
            str(SAMPLES / "tone_vorbis.ogg"),
        ]
    )
    run(
        [
            "ffmpeg",
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=64x48:rate=10:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=660:sample_rate=44100:duration=1",
            "-shortest",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-preset",
            "ultrafast",
            "-tune",
            "zerolatency",
            "-c:a",
            "aac",
            "-b:a",
            "64k",
            "-movflags",
            "+faststart",
            str(SAMPLES / "h264_aac_mp4.mp4"),
        ]
    )


def run(args):
    subprocess.run(args, check=True)


if __name__ == "__main__":
    main()
