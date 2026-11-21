use rustcraft_authority::{Effect, Rejection, ScheduledOperation, Server, TaskEvent};
use rustcraft_host::log_tick;
use rustcraft_world::{BlockPos, ChunkPos};

pub(super) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut server = Server::playable(42)?;
    let empty = server
        .registry()
        .block_id("rustcraft:empty")
        .ok_or("missing empty block")?;
    let basalt = server
        .registry()
        .block_id("rustcraft_content:basalt")
        .ok_or("missing basalt block")?;
    let position = BlockPos::new(2, 0, 2);
    let neighbor = ChunkPos { x: 1, z: 0 };
    let replace = |expected, replacement| ScheduledOperation::ReplaceBlock {
        position,
        expected,
        replacement,
    };
    let placed = server.schedule_operation(2, replace(empty, basalt))?;
    let stale = server.schedule_operation(2, replace(empty, basalt))?;
    let inactive = server.schedule_operation(
        2,
        ScheduledOperation::DeactivateChunk { position: neighbor },
    )?;
    let occupied = server.schedule_operation(
        2,
        ScheduledOperation::DeactivateChunk {
            position: ChunkPos::ORIGIN,
        },
    )?;
    let canceled = server.schedule_task(2)?;
    if !server.cancel_task(canceled) {
        return Err("task demo could not cancel queued task".into());
    }

    let queued = server.step(vec![]);
    log_tick(&queued);
    let due = server.step(vec![]);
    log_tick(&due);
    let results: Vec<_> = due
        .task_events
        .iter()
        .filter_map(|event| match event {
            TaskEvent::Due { task, result, .. } => Some((*task, result.clone())),
            _ => None,
        })
        .collect();
    if !queued.due_tasks.is_empty()
        || due.due_tasks != [placed, stale, inactive, occupied]
        || results
            != [
                (placed, Ok(Effect::ChunkChanged(position.chunk()))),
                (
                    stale,
                    Err(Rejection::UnexpectedBlockState { actual: basalt }),
                ),
                (inactive, Ok(Effect::ChunkChanged(neighbor))),
                (occupied, Err(Rejection::ChunkOccupied)),
            ]
        || due.snapshot.chunks[&position.chunk()].block(position) != Some(basalt)
        || due.snapshot.chunks.contains_key(&neighbor)
    {
        return Err("task demo did not produce the expected operations".into());
    }
    eprintln!("event=task_demo_complete");
    Ok(())
}
