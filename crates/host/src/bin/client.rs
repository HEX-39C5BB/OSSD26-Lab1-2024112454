#[cfg(target_os = "linux")]
#[path = "client/audio.rs"]
mod audio;

#[cfg(target_os = "linux")]
#[path = "client/settings.rs"]
mod settings;

#[cfg(target_os = "linux")]
#[path = "client/viewer.rs"]
mod viewer;

#[cfg(target_os = "linux")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::PathBuf;
    use std::sync::mpsc::TryRecvError;

    use rustcraft_authority::RulePreset;
    use rustcraft_host::{LocalHost, WorldPath, log_tick};

    let mut args = std::env::args_os().skip(1);
    let mut mode = None;
    let mut preview = None;
    let mut world = None;
    let mut open_world = None;
    let mut pack = None;
    let mut settings = None;
    let mut preset = None;
    let mut game_mode = None;
    let mut benchmark_radius = None;
    let mut benchmark_frames = None;
    let mut benchmark_gpu = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some(
                "--probe"
                | "--smoke"
                | "--terrain-smoke"
                | "--audio-smoke"
                | "--preview"
                | "--terrain-benchmark"
                | "--streaming-benchmark",
            ) => {
                if mode.is_some() {
                    return Err("choose one client mode".into());
                }
                if arg == "--preview" {
                    preview = Some(PathBuf::from(
                        args.next().ok_or("--preview requires an output path")?,
                    ));
                }
                if arg == "--terrain-benchmark" || arg == "--streaming-benchmark" {
                    benchmark_radius = Some(
                        args.next()
                            .as_deref()
                            .and_then(std::ffi::OsStr::to_str)
                            .ok_or("benchmark requires a chunk radius")?
                            .parse()?,
                    );
                }
                mode = Some(arg);
            }
            Some("--frames") => {
                benchmark_frames = Some(
                    args.next()
                        .as_deref()
                        .and_then(std::ffi::OsStr::to_str)
                        .ok_or("--frames requires a frame count")?
                        .parse()?,
                );
            }
            Some("--gpu") => {
                benchmark_gpu = Some(
                    match args.next().as_deref().and_then(std::ffi::OsStr::to_str) {
                        Some("intel") => rustcraft_vulkan::BenchmarkGpu::IntelIntegrated,
                        Some("nvidia") => rustcraft_vulkan::BenchmarkGpu::NvidiaDiscrete,
                        Some("auto") => rustcraft_vulkan::BenchmarkGpu::Auto,
                        _ => return Err("--gpu requires intel, nvidia, or auto".into()),
                    },
                );
            }
            Some("--pack") => {
                pack = Some(PathBuf::from(args.next().ok_or("--pack requires a path")?))
            }
            Some("--preset") => {
                let value = args.next().ok_or("--preset requires a value")?;
                preset = Some(RulePreset::from_cli(
                    value.to_str().ok_or("invalid preset name")?,
                )?);
            }
            Some("--gamemode") => {
                let value = args
                    .next()
                    .ok_or("--gamemode requires creative or survival")?;
                game_mode = Some(value.to_str().ok_or("invalid game mode")?.parse()?);
            }
            Some("--world") => {
                world = Some(PathBuf::from(args.next().ok_or("--world requires a path")?))
            }
            Some("--open-world") => {
                open_world = Some(PathBuf::from(
                    args.next().ok_or("--open-world requires a path")?,
                ))
            }
            Some("--settings") => {
                settings = Some(PathBuf::from(
                    args.next().ok_or("--settings requires a path")?,
                ));
            }
            Some("--help") => {
                println!(
                    "Usage: rustcraft-client [--world DIR | --open-world DIR] [--pack FILE] [--settings FILE] [--preset PRESET] [--gamemode creative|survival]\n       rustcraft-client [--probe | --smoke | --terrain-smoke | --audio-smoke | --preview OUTPUT.ppm]\n       rustcraft-client --terrain-benchmark RADIUS [--frames N] [--gpu intel|nvidia|auto]\n       rustcraft-client --streaming-benchmark RADIUS [--gpu intel|nvidia|auto]\n\nPresets: native, java-1.20.1, java-1.20.1-reference, java-1.20.1-reference-v3.\n--open-world requires a published checkpoint.\n--audio-smoke plays a reference tone and the game effects without opening a window."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown client argument {arg:?}").into()),
        }
    }
    if world.is_some() && open_world.is_some() {
        return Err("--world and --open-world are mutually exclusive".into());
    }
    let world = open_world
        .map(WorldPath::Existing)
        .or_else(|| world.map(WorldPath::OpenOrCreate));
    let legacy = mode.as_deref().and_then(std::ffi::OsStr::to_str);
    if (world.is_some() || settings.is_some() || preset.is_some() || game_mode.is_some())
        && mode.is_some()
    {
        return Err(
            "--world, --open-world, --settings, --preset and --gamemode are only available in interactive mode"
                .into(),
        );
    }
    if let Some(radius) = benchmark_radius {
        if pack.is_some() {
            return Err("benchmarks use the built-in resource pack".into());
        }
        let gpu = benchmark_gpu.unwrap_or(rustcraft_vulkan::BenchmarkGpu::IntelIntegrated);
        if legacy == Some("--streaming-benchmark") {
            if benchmark_frames.is_some() {
                return Err("--frames requires --terrain-benchmark".into());
            }
            return viewer::streaming_benchmark(
                u8::try_from(radius)
                    .ok()
                    .and_then(rustcraft_authority::ViewDistance::from_radius)
                    .ok_or("--streaming-benchmark radius must be between 1 and 16")?,
                gpu,
            );
        }
        return viewer::terrain_benchmark(radius, benchmark_frames.unwrap_or(360), gpu);
    }
    if benchmark_frames.is_some() || benchmark_gpu.is_some() {
        return Err("--frames and --gpu require a benchmark mode".into());
    }
    if let Some(preview) = preview {
        return viewer::preview(&preview, pack.as_deref());
    }
    match legacy {
        None => {
            return viewer::run(
                false,
                pack.as_deref(),
                world.as_ref(),
                settings,
                preset,
                game_mode,
            );
        }
        Some("--terrain-smoke") => {
            return viewer::run(true, pack.as_deref(), None, None, None, None);
        }
        Some("--audio-smoke") => return viewer::audio_smoke(pack.as_deref()),
        Some("--probe") if pack.is_none() => return rustcraft_vulkan::probe(),
        Some("--smoke") if pack.is_none() => {}
        _ => return Err("--pack requires a terrain client mode".into()),
    }

    let host = LocalHost::start(0, None)?;
    let result = rustcraft_vulkan::run(true, || {
        loop {
            match host.updates.try_recv() {
                Ok(report) => {
                    if !report.acks.is_empty() || report.snapshot.tick == 1 {
                        log_tick(&report);
                    }
                }
                Err(TryRecvError::Empty) => return Ok(()),
                Err(TryRecvError::Disconnected) => return Err("local authority stopped".into()),
            }
        }
    });
    let shutdown = host.shutdown();
    result.and(shutdown)
}

#[cfg(not(target_os = "linux"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    Err("the Vulkan client requires Linux".into())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("event=client_failed error={error:?}");
            std::process::ExitCode::FAILURE
        }
    }
}
