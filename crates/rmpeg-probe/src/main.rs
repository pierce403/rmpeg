use std::{env, fs, process};

use rmpeg_core::{Result, RmpegError};
use rmpeg_format::parse_wav;

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
    let wav = parse_wav(&input)?;
    let stream = wav.metadata;

    println!("{{");
    println!("  \"format\": \"wav\",");
    println!("  \"streams\": [");
    println!("    {{");
    println!("      \"index\": {},", stream.index);
    println!("      \"codec_type\": \"{}\",", stream.codec_type);
    println!("      \"codec_name\": \"{}\",", stream.codec_name);
    println!("      \"sample_rate\": {},", stream.sample_rate);
    println!("      \"channels\": {},", stream.channels);
    println!("      \"bits_per_sample\": {},", stream.bits_per_sample);
    println!("      \"duration_seconds\": {:.6}", stream.duration_seconds);
    println!("    }}");
    println!("  ]");
    println!("}}");

    Ok(())
}
