use std::error::Error;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use rustcraft_authority::java_block_ticks::ScheduledBlockTick;
use rustcraft_authority::java_bud::{
    BudDiagnosis, BudRecord, BudRecordKind, BudStamp, BudTarget, PistonObstruction,
};
use rustcraft_authority::java_signal::{ActivationPower, Face};
use rustcraft_authority::{JavaBlockTickKind, PistonEvent, Server};
use rustcraft_world::BlockPos;
use serde_json::{Value, json};

pub(super) fn run(command: &str, args: &[String]) -> Result<(), Box<dyn Error>> {
    let (world, x, y, z, ticks) = match (command, args) {
        ("--inspect-bud", [world, x, y, z]) => (world, x, y, z, None),
        ("--trace-bud", [world, x, y, z, ticks]) => (world, x, y, z, Some(ticks.parse::<u64>()?)),
        ("--inspect-bud", _) => return Err("--inspect-bud requires WORLD X Y Z".into()),
        ("--trace-bud", _) => return Err("--trace-bud requires WORLD X Y Z TICKS".into()),
        _ => unreachable!("BUD inspection command"),
    };
    let position = BlockPos::new(x.parse()?, y.parse()?, z.parse()?);
    let (generation, mut server) = rustcraft_persistence::load_existing(Path::new(world))?;
    let initial = server.snapshot();
    if initial
        .tick
        .checked_add(ticks.unwrap_or_default())
        .is_none()
    {
        return Err("trace would exceed the GT counter".into());
    }
    let rules = server.rules().identity();
    let mut output = BufWriter::new(io::stdout().lock());
    line(
        &mut output,
        json!({
            "record": "bud_run",
            "schema": 1,
            "world": world,
            "generation": generation,
            "seed": initial.seed,
            "dimension": server.rules().is_java_flat().then_some("minecraft:overworld"),
            "ruleset": server.rules().uses_java_block_events().then_some("java/1.20.1"),
            "rules": { "id": rules.id, "version": rules.version },
            "start_gt": initial.tick,
            "position": point(position),
            "mode": if ticks.is_some() { "replay" } else { "inspect" },
        }),
    )?;
    if let Some(ticks) = ticks {
        server.watch_java_bud(position);
        let mut sequence = 0;
        emit_trace(&mut output, &server, position, &mut sequence)?;
        for _ in 0..ticks {
            server.step(Vec::new());
            emit_trace(&mut output, &server, position, &mut sequence)?;
        }
    }
    line(&mut output, diagnosis(server.inspect_java_bud(position)))?;
    output.flush()?;
    Ok(())
}

fn emit_trace(
    output: &mut impl Write,
    server: &Server,
    position: BlockPos,
    sequence: &mut u64,
) -> io::Result<()> {
    let seen = *sequence;
    for &record in server
        .java_bud_trace()
        .skip_while(|record| record.stamp.sequence <= seen)
    {
        if record.stamp.sequence != *sequence + 1 {
            line(
                output,
                json!({
                    "record": "bud_trace_gap",
                    "first_missing_sequence": *sequence + 1,
                    "last_missing_sequence": record.stamp.sequence - 1,
                }),
            )?;
        }
        line(output, event(position, record))?;
        *sequence = record.stamp.sequence;
    }
    Ok(())
}

fn point(position: BlockPos) -> [i64; 3] {
    [position.x, position.y, position.z]
}

fn face_name(face: Face) -> &'static str {
    match face {
        Face::North => "north",
        Face::South => "south",
        Face::East => "east",
        Face::West => "west",
        Face::Up => "up",
        Face::Down => "down",
    }
}

fn power(activation: ActivationPower) -> Value {
    match activation {
        ActivationPower::Direct(source) => json!({ "kind": "direct", "source": point(source) }),
        ActivationPower::Quasi(source) => json!({ "kind": "quasi", "source": point(source) }),
        ActivationPower::Unpowered => json!({ "kind": "unpowered" }),
        ActivationPower::Unavailable => json!({ "kind": "unavailable" }),
    }
}

fn target(value: BudTarget) -> Value {
    json!({
        "component": format!("{:?}", value.component),
        "facing": face_name(value.facing),
        "active": value.active,
    })
}

fn stamp(value: BudStamp) -> Value {
    json!({
        "gt": value.gt,
        "phase": format!("{:?}", value.phase),
        "sequence": value.sequence,
    })
}

fn piston(value: PistonEvent) -> Value {
    json!({
        "base": point(value.base),
        "kind": format!("{:?}", value.kind),
        "direction_index": value.facing,
        "action": format!("{:?}", value.action),
        "event_id": value.action.java_event_id(),
    })
}

fn scheduled(value: ScheduledBlockTick<JavaBlockTickKind>) -> Value {
    json!({
        "position": point(value.position),
        "kind": format!("{:?}", value.kind),
        "due_gt": value.due_gt,
        "priority": value.priority as i8,
        "sub_tick_order": value.sub_tick_order,
    })
}

fn obstruction(reason: PistonObstruction) -> Value {
    let (kind, position) = match reason {
        PistonObstruction::WorldBoundary(position) => ("world_boundary", position),
        PistonObstruction::InactiveChunk(position) => ("inactive_chunk", position),
        PistonObstruction::Player(position) => ("player", position),
        PistonObstruction::PushLimit(position) => ("push_limit", position),
        PistonObstruction::Immovable { position, state } => {
            return json!({ "kind": "immovable", "position": point(position), "state": state });
        }
    };
    json!({ "kind": kind, "position": point(position) })
}

fn event(position: BlockPos, value: BudRecord) -> Value {
    let (kind, details) = match value.kind {
        BudRecordKind::Observed {
            state,
            target: block,
            activation,
            status,
        } => (
            "observed",
            json!({ "state": state, "target": block.map(target), "activation": activation.map(power), "status": status.name() }),
        ),
        BudRecordKind::Edit { before, after } => {
            ("block_edit", json!({ "before": before, "after": after }))
        }
        BudRecordKind::Shape { from } => (
            "shape",
            json!({ "direction": face_name(from), "source": from.offset(position).map(point) }),
        ),
        BudRecordKind::Check { source, activation } => (
            "activation_check",
            json!({ "source": source.map(point), "activation": power(activation) }),
        ),
        BudRecordKind::BlockEvent(trace) => (
            "block_event",
            json!({
                "stage": format!("{:?}", trace.stage),
                "action": piston(trace.event),
                "source": trace.source.map(point),
                "moving_position": trace.moving_position.map(point),
                "activation": trace.power.map(power),
            }),
        ),
        BudRecordKind::Scheduled { tick, inserted } => (
            "schedule",
            json!({ "tick": scheduled(tick), "inserted": inserted }),
        ),
        BudRecordKind::Callback { tick, outcome } => (
            "callback",
            json!({ "tick": scheduled(tick), "outcome": format!("{outcome:?}") }),
        ),
    };
    json!({
        "record": "bud_event",
        "position": point(position),
        "stamp": stamp(value.stamp),
        "parent_sequence": value.parent_sequence,
        "event": kind,
        "details": details,
    })
}

fn diagnosis(report: BudDiagnosis) -> Value {
    json!({
        "record": "bud_diagnosis",
        "gt": report.gt,
        "phase": "Boundary",
        "position": point(report.position),
        "state": report.state,
        "target": report.target.map(target),
        "activation": report.activation.map(power),
        "status": report.status.name(),
        "obstruction": report.obstruction.map(obstruction),
        "piston_events": report.piston_events.into_iter().map(piston).collect::<Vec<_>>(),
        "scheduled_ticks": report.scheduled_ticks.into_iter().map(scheduled).collect::<Vec<_>>(),
        "motion": report.motion.into_iter().map(|motion| json!({
            "action": piston(motion.event),
            "position": point(motion.position),
            "remaining_gt": motion.remaining,
            "previous_remaining_gt": motion.previous_remaining,
            "awaiting_activation": motion.awaiting_activation,
        })).collect::<Vec<_>>(),
        "last_check": report.last_check.map(|record| event(report.position, record)),
        "last_block_event": report.last_block_event.map(|record| event(report.position, record)),
        "last_scheduled_event": report.last_scheduled_event.map(|record| event(report.position, record)),
        "missing_baseline": report.missing_baseline.map(stamp),
        "missing_since": report.missing_since.map(stamp),
    })
}

fn line(output: &mut impl Write, value: Value) -> io::Result<()> {
    serde_json::to_writer(&mut *output, &value)?;
    writeln!(output)
}
