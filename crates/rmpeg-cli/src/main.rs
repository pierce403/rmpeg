use std::{env, fs, process};

use rmpeg_codec::pcm_s16le_frame_hashes;
use rmpeg_core::{Result, RmpegError};
use rmpeg_format::parse_wav;

fn main() {
    if let Err(error) = run() {
        eprintln!("rmpeg: {error}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 || args[0] != "decode" || args[2] != "--framemd5" {
        return Err(RmpegError::Usage(
            "usage: rmpeg decode <input.wav> --framemd5".to_string(),
        ));
    }

    let input = fs::read(&args[1])?;
    let wav = parse_wav(&input)?;
    let frames = pcm_s16le_frame_hashes(&input, &wav, 1024)?;

    println!("#format: frame checksums");
    println!("#version: 2");
    println!("#hash: MD5");
    println!("#software: rmpeg");
    println!("#tb 0: 1/{}", wav.metadata.sample_rate);
    println!("#media_type 0: audio");
    println!("#codec_id 0: pcm_s16le");
    println!("#sample_rate 0: {}", wav.metadata.sample_rate);
    println!("#channels 0: {}", wav.metadata.channels);
    println!("#stream#, dts, pts, duration, size, hash");
    for frame in frames {
        println!(
            "{}, {}, {}, {}, {}, {}",
            frame.stream_index, frame.dts, frame.pts, frame.duration, frame.size, frame.hash
        );
    }

    Ok(())
}
