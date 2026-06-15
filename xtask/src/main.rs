use std::{
    env, io,
    path::PathBuf,
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<()> {
    let mut args = env::args().skip(1);
    let task = args.next().unwrap_or_else(|| "help".to_string());
    if args.next().is_some() {
        return Err(io::Error::other("xtask accepts exactly one task"));
    }

    match task.as_str() {
        "samples" => python(&["harness/scripts/generate_samples.py"]),
        "reference" => python(&["harness/scripts/run_mirrored_tests.py", "reference"]),
        "fate-mini" => {
            build_release()?;
            python(&["harness/scripts/run_mirrored_tests.py", "run"])
        }
        "bench" => {
            build_release()?;
            python(&["harness/scripts/run_benchmarks.py"])
        }
        "site" => python(&["harness/scripts/render_site.py"]),
        "ffprobe-oracle" => python(&["harness/scripts/run_ffmpeg_samples.py", "ffprobe-oracle"]),
        "ffmpeg-samples-sync" => python(&["harness/scripts/run_ffmpeg_samples.py", "sync"]),
        "ffmpeg-samples-check" => {
            build_release()?;
            python(&["harness/scripts/run_ffmpeg_samples.py", "check"])
        }
        "ffmpeg-samples" => {
            build_release()?;
            python(&["harness/scripts/run_ffmpeg_samples.py", "all"])
        }
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(io::Error::other(format!("unknown xtask task: {other}"))),
    }
}

fn print_help() {
    println!("usage: cargo xtask <task>");
    println!();
    println!("tasks:");
    println!("  samples    generate deterministic WAV fixtures");
    println!("  reference  generate FFmpeg/ffprobe reference outputs");
    println!("  fate-mini  run mirrored correctness tests");
    println!("  bench      run hyperfine benchmarks");
    println!("  site       render site/dist/index.html");
    println!("  ffprobe-oracle        build the pinned ffprobe oracle for upstream samples");
    println!("  ffmpeg-samples-sync   sync upstream FFmpeg FATE samples with make fate-rsync");
    println!("  ffmpeg-samples-check  probe the synced FFmpeg FATE corpus and inventory decode execution");
    println!("  ffmpeg-samples        sync, probe, and inventory the FFmpeg FATE sample corpus");
}

fn build_release() -> io::Result<()> {
    command(
        "cargo",
        &["build", "--release", "-p", "rmpeg-probe", "-p", "rmpeg-cli"],
    )
}

fn python(args: &[&str]) -> io::Result<()> {
    command("python3", args)
}

fn command(program: &str, args: &[&str]) -> io::Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(root())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{program} {} exited with {status}",
            args.join(" ")
        )))
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate has a workspace parent")
        .to_path_buf()
}
