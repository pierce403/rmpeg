use std::{env, fs, process};

use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};
use rmpeg_format::probe;

fn main() {
    if let Err(error) = run() {
        eprintln!("rmpeg-probe: {error}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| RmpegError::Usage("usage: rmpeg-probe <input.wav>".to_string()))?;
    if args.next().is_some() {
        return Err(RmpegError::Usage(
            "usage: rmpeg-probe <input.wav>".to_string(),
        ));
    }

    let input = fs::read(&path)?;
    print_probe_json(&probe(&input)?);

    Ok(())
}

fn print_probe_json(document: &ProbeDocument) {
    println!("{{");
    println!("  \"format\": \"{}\",", escape_json(&document.format));
    println!("  \"streams\": [");
    for (index, stream) in document.streams.iter().enumerate() {
        print_stream(stream, index + 1 == document.streams.len());
    }
    println!("  ]");
    println!("}}");
}

fn print_stream(stream: &StreamMetadata, is_last: bool) {
    let mut fields = Vec::new();
    fields.push(format!("\"index\": {}", stream.index));
    fields.push(format!(
        "\"codec_type\": \"{}\"",
        escape_json(&stream.codec_type)
    ));
    fields.push(format!(
        "\"codec_name\": \"{}\"",
        escape_json(&stream.codec_name)
    ));
    if let Some(sample_rate) = stream.sample_rate {
        fields.push(format!("\"sample_rate\": {sample_rate}"));
    }
    if let Some(channels) = stream.channels {
        fields.push(format!("\"channels\": {channels}"));
    }
    if let Some(bits_per_sample) = stream.bits_per_sample {
        fields.push(format!("\"bits_per_sample\": {bits_per_sample}"));
    }
    if let Some(duration_seconds) = stream.duration_seconds {
        fields.push(format!("\"duration_seconds\": {duration_seconds:.6}"));
    }
    if let Some(width) = stream.width {
        fields.push(format!("\"width\": {width}"));
    }
    if let Some(height) = stream.height {
        fields.push(format!("\"height\": {height}"));
    }
    if let Some(frame_rate) = &stream.frame_rate {
        fields.push(format!("\"frame_rate\": \"{}\"", escape_json(frame_rate)));
    }

    println!("    {{");
    for (index, field) in fields.iter().enumerate() {
        let comma = if index + 1 == fields.len() { "" } else { "," };
        println!("      {field}{comma}");
    }
    println!("    }}{}", if is_last { "" } else { "," });
}

fn escape_json(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
