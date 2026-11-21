use std::time::{Duration, Instant};

use rustcraft_authority::{BlockAction, Effect, Mutation, PlayerInput, Snapshot, TickReport};
use rustcraft_host::{LocalHost, log_tick};

type Error = Box<dyn std::error::Error>;
const REPORT_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) fn run(seed: u64) -> Result<(), Error> {
    let host = LocalHost::start_playable(seed)?;
    let played = play(&host);
    let stopped = host.shutdown();
    played.and(stopped)?;
    eprintln!("event=world_stopped");
    Ok(())
}

fn play(host: &LocalHost) -> Result<(), Error> {
    let landed = wait_for(host, |report| {
        report.snapshot.player.is_some_and(|player| player.grounded)
    })?;
    let origin = landed.snapshot.player.ok_or("player unavailable")?.position;
    let walk = Mutation::ControlPlayer(PlayerInput {
        forward: 1,
        ..PlayerInput::default()
    });
    let start = apply(host, 1, walk, Effect::PlayerInputAccepted)?;
    wait_for(host, |report| report.snapshot.tick >= start.tick + 4)?;

    let aim = Mutation::ControlPlayer(PlayerInput {
        pitch: -0.65,
        ..PlayerInput::default()
    });
    let aimed = apply(host, 2, aim, Effect::PlayerInputAccepted)?;
    let player = aimed.player.ok_or("player unavailable")?;
    if player.position[2] <= origin[2] {
        return Err("player did not advance".into());
    }
    eprintln!(
        "event=player_moved from={origin:?} to={:?}",
        player.position
    );

    let target = player.target.ok_or("player did not target the ground")?;
    let place = target
        .placement
        .ok_or("ground has no placement space")?
        .position;
    let chunk = place.chunk();
    let before = aimed.chunks[&chunk].block(place);
    let placed = apply(
        host,
        3,
        Mutation::UseBlock {
            action: BlockAction::Place,
            target,
        },
        Effect::ChunkChanged(chunk),
    )?;
    let after = placed.chunks[&chunk].block(place);
    if before == after {
        return Err("placement did not change the block".into());
    }
    eprintln!("event=block_placed position={place:?}");

    let target = placed
        .player
        .and_then(|player| player.target)
        .ok_or("player lost the placed block")?;
    if target.position != place {
        return Err("player is targeting a different block".into());
    }
    let broken = apply(
        host,
        4,
        Mutation::UseBlock {
            action: BlockAction::Break,
            target,
        },
        Effect::ChunkChanged(chunk),
    )?;
    if broken.chunks[&chunk].block(place) != before {
        return Err("breaking did not restore the empty cell".into());
    }
    eprintln!("event=block_broken position={place:?}");
    eprintln!("event=p2_demo_complete");
    Ok(())
}

fn apply(
    host: &LocalHost,
    operation: u64,
    mutation: Mutation,
    expected: Effect,
) -> Result<Snapshot, Error> {
    host.submit(operation, mutation)?;
    let report = wait_for(host, |report| {
        report.acks.iter().any(|ack| ack.operation == operation)
    })?;
    let ack = report
        .acks
        .iter()
        .find(|ack| ack.operation == operation)
        .expect("matching acknowledgement");
    if ack.result != Ok(expected) {
        return Err(format!("operation {operation} returned {:?}", ack.result).into());
    }
    log_tick(&report);
    Ok(report.snapshot)
}

fn wait_for(host: &LocalHost, accepts: impl Fn(&TickReport) -> bool) -> Result<TickReport, Error> {
    let deadline = Instant::now() + REPORT_TIMEOUT;
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("tick report timed out")?;
        let report = host.updates.recv_timeout(remaining)?;
        if accepts(&report) {
            return Ok(report);
        }
    }
}
