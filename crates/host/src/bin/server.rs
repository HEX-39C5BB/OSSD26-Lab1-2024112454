use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rustcraft_authority::{GameMode, RulePreset, Server};
use rustcraft_host::{LocalHost, WorldPath, log_tick};
use rustcraft_world::VerticalRange;

#[path = "server/benchmark.rs"]
mod benchmark;
#[path = "server/bud.rs"]
mod bud;
#[path = "server/demo.rs"]
mod demo;
#[path = "server/task_commands.rs"]
mod task_commands;
#[path = "server/task_demo.rs"]
mod task_demo;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut seed = None;
    let mut ticks = None;
    let mut demo = false;
    let mut benchmark = false;
    let mut radius = None;
    let mut world = None;
    let mut open_world = None;
    let mut height = None;
    let mut preset = None;
    let mut game_mode = None;
    let mut args = std::env::args().skip(1).peekable();
    if matches!(
        args.peek().map(String::as_str),
        Some("--inspect-bud" | "--trace-bud")
    ) {
        let command = args.next().unwrap();
        return bud::run(&command, &args.collect::<Vec<_>>());
    }
    if matches!(
        args.peek().map(String::as_str),
        Some("--schedule-replace-block" | "--schedule-deactivate-chunk" | "--cancel-task")
    ) {
        let command = args.next().expect("scheduled task command");
        return task_commands::write(&command, &args.collect::<Vec<_>>());
    }
    if args.peek().is_some_and(|flag| flag == "--task-demo") {
        args.next();
        if args.next().is_some() {
            return Err("--task-demo takes no arguments".into());
        }
        return task_demo::run();
    }
    if matches!(
        args.peek().map(String::as_str),
        Some("--verify-world" | "--list-checkpoints" | "--list-tasks")
    ) {
        let command = args.next().expect("checkpoint inspection command");
        let world = args
            .next()
            .ok_or_else(|| format!("{command} requires WORLD"))?;
        if args.next().is_some() {
            return Err(format!("{command} takes exactly one directory argument").into());
        }
        let world = Path::new(&world);
        return match command.as_str() {
            "--list-checkpoints" => list_checkpoints(world),
            "--list-tasks" => task_commands::list(world),
            _ => verify_world(world, None),
        };
    }
    if args
        .peek()
        .is_some_and(|flag| flag == "--verify-generation")
    {
        args.next();
        let (Some(world), Some(generation), None) = (args.next(), args.next(), args.next()) else {
            return Err("--verify-generation requires exactly WORLD GENERATION".into());
        };
        return verify_world(Path::new(&world), Some(parse_generation(&generation)?));
    }
    if args
        .peek()
        .is_some_and(|flag| flag == "--restore-generation")
    {
        args.next();
        let (Some(world), Some(generation), Some(destination), None) =
            (args.next(), args.next(), args.next(), args.next())
        else {
            return Err("--restore-generation requires exactly WORLD GENERATION NEW_DIR".into());
        };
        let generation = parse_generation(&generation)?;
        rustcraft_persistence::restore_generation(
            Path::new(&world),
            generation,
            Path::new(&destination),
        )?;
        eprintln!("event=checkpoint_restored generation={generation} destination={destination:?}");
        return Ok(());
    }
    if matches!(
        args.peek().map(String::as_str),
        Some("--backup" | "--restore" | "--migrate-world")
    ) {
        let command = args.next().expect("checkpoint transfer command");
        let source = args
            .next()
            .ok_or_else(|| format!("{command} requires SOURCE DESTINATION"))?;
        let destination = args
            .next()
            .ok_or_else(|| format!("{command} requires SOURCE DESTINATION"))?;
        if args.next().is_some() {
            return Err(format!("{command} takes exactly two directory arguments").into());
        }
        let source = Path::new(&source);
        let destination = Path::new(&destination);
        let (event, generation) = match command.as_str() {
            "--backup" => (
                "checkpoint_backup_created",
                rustcraft_persistence::copy_checkpoint(source, destination)?,
            ),
            "--restore" => (
                "checkpoint_restored",
                rustcraft_persistence::copy_checkpoint(source, destination)?,
            ),
            "--migrate-world" => (
                "checkpoint_migrated",
                rustcraft_persistence::migrate_checkpoint(source, destination)?,
            ),
            _ => unreachable!("checkpoint transfer command"),
        };
        eprintln!("event={event} generation={generation} destination={destination:?}");
        return Ok(());
    }
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--help" => {
                println!(
                    "Usage: rustcraft-server [--seed N] [--ticks N | --demo]
       rustcraft-server (--world DIR | --open-world DIR) [--seed N] [--height MIN MAX_EXCLUSIVE] [--preset PRESET] [--gamemode creative|survival] [--ticks N]
       rustcraft-server --verify-world WORLD
       rustcraft-server --verify-generation WORLD GENERATION
       rustcraft-server --list-checkpoints WORLD
       rustcraft-server --list-tasks WORLD
       rustcraft-server --inspect-bud WORLD X Y Z
       rustcraft-server --trace-bud WORLD X Y Z TICKS
       rustcraft-server --schedule-replace-block WORLD DELAY_TICKS X Y Z EXPECTED REPLACEMENT
       rustcraft-server --schedule-deactivate-chunk WORLD DELAY_TICKS CHUNK_X CHUNK_Z
       rustcraft-server --cancel-task WORLD TASK_ID
       rustcraft-server --backup WORLD NEW_DIR
       rustcraft-server --restore BACKUP NEW_DIR
       rustcraft-server --restore-generation WORLD GENERATION NEW_DIR
       rustcraft-server --migrate-world WORLD NEW_DIR
       rustcraft-server --task-demo
       rustcraft-server --benchmark [--seed N] [--ticks N] [--radius CHUNKS]

Presets: native, java-1.20.1, java-1.20.1-reference, java-1.20.1-reference-v3.
--open-world requires a published checkpoint."
                );
                return Ok(());
            }
            "--seed" => seed = Some(args.next().ok_or("--seed requires a value")?.parse()?),
            "--ticks" => ticks = Some(args.next().ok_or("--ticks requires a value")?.parse()?),
            "--preset" => {
                preset = Some(RulePreset::from_cli(
                    &args.next().ok_or("--preset requires a value")?,
                )?)
            }
            "--gamemode" => {
                game_mode = Some(
                    args.next()
                        .ok_or("--gamemode requires creative or survival")?
                        .parse()?,
                )
            }
            "--demo" => demo = true,
            "--benchmark" => benchmark = true,
            "--radius" => {
                radius = Some(args.next().ok_or("--radius requires a value")?.parse()?);
            }
            "--world" => {
                world = Some(PathBuf::from(
                    args.next().ok_or("--world requires a directory")?,
                ))
            }
            "--open-world" => {
                open_world = Some(PathBuf::from(
                    args.next().ok_or("--open-world requires a directory")?,
                ))
            }
            "--height" => {
                height = Some(VerticalRange {
                    min: args
                        .next()
                        .ok_or("--height requires MIN MAX_EXCLUSIVE")?
                        .parse()?,
                    max_exclusive: args
                        .next()
                        .ok_or("--height requires MIN MAX_EXCLUSIVE")?
                        .parse()?,
                });
            }
            _ => return Err(format!("unknown argument {flag}").into()),
        }
    }
    if world.is_some() && open_world.is_some() {
        return Err("--world and --open-world are mutually exclusive".into());
    }
    let world = open_world
        .map(WorldPath::Existing)
        .or_else(|| world.map(WorldPath::OpenOrCreate));
    if benchmark {
        if world.is_some() || height.is_some() || preset.is_some() || game_mode.is_some() || demo {
            return Err("--benchmark accepts only --seed, --ticks and --radius".into());
        }
        return benchmark::run(
            seed.unwrap_or(42),
            ticks.unwrap_or(2048),
            radius.unwrap_or(1),
        );
    }
    if radius.is_some() {
        return Err("--radius requires --benchmark".into());
    }
    if let Some(world) = world {
        if demo {
            return Err("--demo cannot be combined with --world or --open-world".into());
        }
        return run_world(&world, seed, height, preset, game_mode, ticks.unwrap_or(5));
    }
    if height.is_some() {
        return Err("--height requires --world or --open-world".into());
    }
    if preset.is_some() {
        return Err("--preset requires --world or --open-world".into());
    }
    if game_mode.is_some() {
        return Err("--gamemode requires --world or --open-world".into());
    }
    if demo {
        if ticks.is_some() {
            return Err("--demo and --ticks are mutually exclusive".into());
        }
        return demo::run(seed.unwrap_or_default());
    }
    let host = LocalHost::start(seed.unwrap_or_default(), Some(ticks.unwrap_or(5)))?;
    for report in &host.updates {
        log_tick(&report);
    }
    host.shutdown()?;
    eprintln!("event=world_stopped");
    Ok(())
}

fn list_checkpoints(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let list = rustcraft_persistence::list_recovery_candidates(path)?;
    let pointer = match &list.current {
        Ok(Some(generation)) => generation.to_string(),
        Ok(None) => "missing".into(),
        Err(error) => {
            eprintln!("event=checkpoint_pointer_unreadable error={error:?}");
            "unreadable".into()
        }
    };
    println!(
        "event=checkpoint_candidates current={pointer} count={}",
        list.entries.len()
    );
    for entry in list.entries {
        let generation = entry.generation;
        let current = match &list.current {
            Ok(Some(active)) if *active == generation => "true",
            Ok(Some(_)) => "false",
            _ => "unknown",
        };
        match entry.tick {
            Ok(tick) => println!(
                "event=checkpoint_candidate generation={generation} tick={tick} current={current} status=manifest_ok"
            ),
            Err(error) => println!(
                "event=checkpoint_candidate generation={generation} tick=unknown current={current} status=manifest_error error={error:?}"
            ),
        }
    }
    Ok(())
}

fn parse_generation(input: &str) -> Result<u64, &'static str> {
    input
        .parse::<std::num::NonZeroU64>()
        .map(std::num::NonZeroU64::get)
        .map_err(|_| "GENERATION must be a positive integer")
}

fn verify_world(path: &Path, generation: Option<u64>) -> Result<(), Box<dyn std::error::Error>> {
    let (generation, server) = match generation {
        Some(generation) => (
            generation,
            rustcraft_persistence::load_generation(path, generation)?,
        ),
        None => rustcraft_persistence::load_existing(path)?,
    };
    let snapshot = server.snapshot();
    let rules = server.rules();
    let preset = rules.identity();
    let generator = rules.world_generator();
    let schedule = rules.tick_schedule();
    let movement = rules.player_movement();
    let interaction = rules.block_inventory();
    let seeds = rules.seed_derivation();
    let random = rules.random_stream();
    println!(
        "event=checkpoint_verified generation={generation} tick={} seed={} active_chunks={} \
         preset={}@{} world_generator={}@{} tick_schedule={}@{} player_movement={}@{} \
         block_inventory={}@{} seed_derivation={}@{} random_stream={}@{} path={path:?}",
        snapshot.tick,
        snapshot.seed,
        snapshot.chunks.len(),
        preset.id,
        preset.version,
        generator.id,
        generator.version,
        schedule.id,
        schedule.version,
        movement.id,
        movement.version,
        interaction.id,
        interaction.version,
        seeds.id,
        seeds.version,
        random.id,
        random.version,
    );
    Ok(())
}

fn run_world(
    world: &WorldPath,
    seed: Option<u64>,
    height: Option<VerticalRange>,
    preset: Option<RulePreset>,
    game_mode: Option<GameMode>,
    ticks: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let (generation, mut server) = match world.load()? {
        Some((generation, server)) => {
            let loaded = server.snapshot();
            if seed.is_some_and(|seed| seed != loaded.seed)
                || height.is_some_and(|height| Some(height) != loaded.height)
                || preset.is_some_and(|preset| preset != server.rules())
            {
                return Err("seed, height, or preset conflicts with the existing world".into());
            }
            eprintln!(
                "event=checkpoint_loaded generation={generation} tick={}",
                loaded.tick
            );
            (Some(generation), server)
        }
        None => {
            let preset = preset.unwrap_or(RulePreset::NativeV1);
            (
                None,
                Server::playable_with_rules(
                    seed.unwrap_or_default(),
                    height.unwrap_or(preset.default_range()),
                    preset,
                )?,
            )
        }
    };
    let mode = server.configure_game_mode(game_mode);
    eprintln!("event=game_mode mode={}", mode.name());
    let host = LocalHost::start_world(server, Some(ticks))?;
    for report in &host.updates {
        log_tick(&report);
    }
    let server = host.finish()?;
    let saved = rustcraft_persistence::save(world.as_path(), generation, &server)?;
    eprintln!("event=checkpoint_saved generation={saved}");
    eprintln!("event=world_stopped");
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("event=server_failed error={error:?}");
            ExitCode::FAILURE
        }
    }
}
