use std::error::Error;
use std::num::NonZeroU64;
use std::path::Path;
use std::str::FromStr;

use rustcraft_authority::{ScheduledOperation, ScheduledTask, Server};
use rustcraft_persistence::{load_existing, save};
use rustcraft_world::{BlockPos, ChunkPos};

type CommandError = Box<dyn Error>;

pub(super) fn write(command: &str, args: &[String]) -> Result<(), CommandError> {
    match (command, args) {
        ("--schedule-replace-block", [world, delay, x, y, z, expected, replacement]) => {
            let delay = number::<NonZeroU64>(delay, "DELAY_TICKS")?.get();
            let position = BlockPos::new(number(x, "X")?, number(y, "Y")?, number(z, "Z")?);
            persist_edit(Path::new(world), "task_queued", |server| {
                let state = |name: &str| {
                    server
                        .registry()
                        .block_id(name)
                        .ok_or_else(|| format!("unknown block state {name:?}"))
                };
                let operation = ScheduledOperation::ReplaceBlock {
                    position,
                    expected: state(expected)?,
                    replacement: state(replacement)?,
                };
                Ok(server.schedule_operation(delay, operation)?)
            })
        }
        ("--schedule-deactivate-chunk", [world, delay, x, z]) => {
            let delay = number::<NonZeroU64>(delay, "DELAY_TICKS")?.get();
            let position = ChunkPos {
                x: number(x, "CHUNK_X")?,
                z: number(z, "CHUNK_Z")?,
            };
            persist_edit(Path::new(world), "task_queued", |server| {
                Ok(server
                    .schedule_operation(delay, ScheduledOperation::DeactivateChunk { position })?)
            })
        }
        ("--cancel-task", [world, id]) => {
            let id = number::<NonZeroU64>(id, "TASK_ID")?.get();
            persist_edit(Path::new(world), "task_canceled", |server| {
                let (task, _) = server
                    .scheduled_tasks()
                    .find(|(task, _)| task.id == id)
                    .ok_or_else(|| format!("no pending task with id {id}"))?;
                if !server.cancel_task(task) {
                    return Err(format!("could not cancel pending task {id}").into());
                }
                Ok(task)
            })
        }
        ("--schedule-replace-block", _) => Err(
            "--schedule-replace-block requires WORLD DELAY_TICKS X Y Z EXPECTED REPLACEMENT".into(),
        ),
        ("--schedule-deactivate-chunk", _) => {
            Err("--schedule-deactivate-chunk requires WORLD DELAY_TICKS CHUNK_X CHUNK_Z".into())
        }
        ("--cancel-task", _) => Err("--cancel-task requires WORLD TASK_ID".into()),
        _ => unreachable!("scheduled task command"),
    }
}

fn number<T: FromStr>(input: &str, name: &str) -> Result<T, CommandError> {
    input
        .parse()
        .map_err(|_| format!("invalid {name}: {input:?}").into())
}

fn persist_edit(
    world: &Path,
    event: &str,
    edit: impl FnOnce(&mut Server) -> Result<ScheduledTask, CommandError>,
) -> Result<(), CommandError> {
    let (generation, mut server) = load_existing(world)?;
    let task = edit(&mut server)?;
    let generation = save(world, Some(generation), &server)?;
    println!(
        "event={event} generation={generation} id={} due_tick={}",
        task.id, task.due_tick
    );
    Ok(())
}

pub(super) fn list(world: &Path) -> Result<(), CommandError> {
    let (generation, server) = load_existing(world)?;
    let tick = server.snapshot().tick;
    let tasks = server.scheduled_tasks();
    println!(
        "event=scheduled_tasks generation={generation} tick={tick} pending={}",
        tasks.len()
    );
    for (task, operation) in tasks {
        let prefix = format!(
            "event=scheduled_task id={} due_tick={} remaining_ticks={} kind={}",
            task.id,
            task.due_tick,
            task.due_tick - tick,
            operation.name()
        );
        match operation {
            ScheduledOperation::Event => println!("{prefix}"),
            ScheduledOperation::RedstoneCheck { position } => println!(
                "{prefix} x={} y={} z={}",
                position.x, position.y, position.z
            ),
            ScheduledOperation::DeactivateChunk { position } => {
                println!("{prefix} chunk_x={} chunk_z={}", position.x, position.z)
            }
            ScheduledOperation::ReplaceBlock {
                position,
                expected,
                replacement,
            } => {
                let expected = server
                    .registry()
                    .block(expected)
                    .expect("validated scheduled block");
                let replacement = server
                    .registry()
                    .block(replacement)
                    .expect("validated scheduled block");
                println!(
                    "{prefix} x={} y={} z={} expected={:?} replacement={:?}",
                    position.x, position.y, position.z, expected.name, replacement.name
                );
            }
        }
    }
    for task in server.pending_java_block_ticks() {
        let pos = task.position;
        println!(
            "event=java_block_tick due_gt={} priority={} order={} kind={:?} x={} y={} z={}",
            task.due_gt, task.priority as i8, task.sub_tick_order, task.kind, pos.x, pos.y, pos.z
        );
    }
    for event in server.pending_piston_events() {
        let base = event.base;
        println!(
            "event=pending_piston_event action={:?} facing={} x={} y={} z={}",
            event.action, event.facing, base.x, base.y, base.z,
        );
    }
    Ok(())
}
