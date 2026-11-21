use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader};

use rustcraft_authority::java_block_ticks::{
    BlockTickPriority, JavaBlockTicks, ScheduledBlockTick,
};
use rustcraft_authority::java_signal::{
    Emitter, Face, PistonPower, PowerContext, Signal, SignalCell, WirePorts, piston_power,
    power_output,
};
use rustcraft_authority::{
    BlockAction, Capabilities, Coordinate, InventoryAction, JavaBlockTickKind, JavaNotification,
    Mutation, PistonKind, PlayerCommand, Rejection, Request, RulePreset, Server, SessionId,
};
use rustcraft_world::{BlockPos, ChunkPos};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
enum Reference {
    #[serde(rename = "reference_run")]
    Run {
        schema: u32,
        ruleset: String,
        client_sha256: String,
    },
    ScheduleAttempt {
        pos: [i64; 3],
        due_gt: u64,
        priority: i8,
        sub_tick_order: u64,
        kind: String,
    },
    Order {
        fixture: String,
        drain_gt: Option<u64>,
        values: Vec<String>,
    },
    Input {
        fixture: String,
        gt: u64,
        pos: [i64; 3],
        command: Option<String>,
        operation: Option<String>,
    },
    Sample {
        fixture: String,
        gt: u64,
        pos: [i64; 3],
        state: Option<String>,
    },
    PowerQuery {
        fixture: String,
        gt: u64,
        pos: [i64; 3],
        toward: String,
        signal: ReferenceSignal,
    },
    PistonPowerQuery {
        gt: u64,
        facing: String,
        offset: [i64; 3],
        powered: bool,
    },
    BlockEventQueue {
        fixture: String,
        gt: u64,
        pos: [i64; 3],
        pending: Vec<ReferencePistonEvent>,
    },
    BlockEventOrder {
        fixture: String,
        gt: u64,
        phase: String,
        probes: Vec<[i64; 3]>,
        pending: Vec<ReferencePistonEventAt>,
    },
    BlockTickQueue {
        fixture: String,
        gt: u64,
        pos: [i64; 3],
        pending: Vec<ReferenceBlockTick>,
    },
    EjectorInventory {
        fixture: String,
        gt: u64,
        pos: [i64; 3],
        slots: Vec<ReferenceEjectorSlot>,
    },
    EjectedItems {
        fixture: String,
        gt: u64,
        pos: [i64; 3],
        items: Vec<ReferenceEjectedItem>,
    },
    ObserverPlacement {
        phase: String,
        state: String,
    },
    LeverPlacement {
        phase: String,
        state: String,
    },
    TorchAttachment {
        phase: String,
        pos: [i64; 3],
        state: String,
    },
    WorldReloaded {
        fixture: String,
        gt: u64,
    },
    ChunkActivity {
        fixture: String,
        gt: u64,
        chunk: [i64; 2],
        active: bool,
        resident: bool,
    },
    PistonProgress {
        fixture: String,
        gt: u64,
        pos: [i64; 3],
        base: [i64; 3],
        previous: f32,
        current: f32,
        block_ticking: bool,
        entities_loaded: bool,
    },
    Complete,
    #[serde(other)]
    Other,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct ReferenceSignal {
    normal: u8,
    direct: u8,
    emitted: u8,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct ReferencePistonEvent {
    block: String,
    id: u8,
    data: u8,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct ReferencePistonEventAt {
    pos: [i64; 3],
    block: String,
    id: u8,
    data: u8,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct ReferenceBlockTick {
    block: String,
    due_gt: u64,
    priority: i8,
    sub_tick_order: i64,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct ReferenceEjectorSlot {
    slot: usize,
    count: u16,
    item: String,
}

#[derive(Deserialize)]
struct ReferenceEjectedItem {
    count: u16,
    item: String,
    position: [f64; 3],
    age: u16,
}

fn load_reference() -> Vec<Reference> {
    let path = std::env::var_os("RUSTCRAFT_JAVA_REDSTONE_REFERENCE")
        .expect("path to reference.jsonl from tools/run_java_redstone_reference.py");
    let file = File::open(path).expect("open original reference trace");
    let reference: Vec<Reference> = BufReader::new(file)
        .lines()
        .map(|line| {
            serde_json::from_str(&line.expect("read reference line")).expect("parse reference line")
        })
        .collect();
    let Some(Reference::Run {
        schema,
        ruleset,
        client_sha256,
    }) = reference.first()
    else {
        panic!("missing reference provenance");
    };
    assert_eq!(*schema, 1);
    assert_eq!(ruleset, "java/1.20.1");
    assert_eq!(
        client_sha256,
        "56b71336d2b4fdffd197f56595b0da93e32a946f78f382a299b8f4b92758bb0f"
    );
    assert!(matches!(reference.last(), Some(Reference::Complete)));
    reference
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_original_java_chunk_tick_order() {
    let reference = load_reference();
    let mut block_ticks = JavaBlockTicks::default();
    let mut observed = false;
    for record in &reference {
        match record {
            Reference::ScheduleAttempt {
                pos: [x, y, z],
                due_gt,
                priority,
                sub_tick_order,
                kind,
            } => {
                use BlockTickPriority::*;
                let priority = match priority {
                    -3 => ExtremelyHigh,
                    -2 => VeryHigh,
                    -1 => High,
                    0 => Normal,
                    1 => Low,
                    2 => VeryLow,
                    3 => ExtremelyLow,
                    priority => panic!("unexpected priority {priority}"),
                };
                block_ticks.schedule(ScheduledBlockTick {
                    due_gt: *due_gt,
                    priority,
                    sub_tick_order: *sub_tick_order,
                    position: BlockPos::new(*x, *y, *z),
                    kind: kind.as_str(),
                });
            }
            Reference::Order {
                fixture,
                drain_gt,
                values,
            } if fixture == "ORDER-CHUNK" => {
                let gt = drain_gt.expect("GT used to drain the original scheduler");
                let actual: Vec<_> = block_ticks
                    .collect_due(gt, 10, |_| true)
                    .into_iter()
                    .map(|tick| tick.kind)
                    .collect();
                assert!(!values.is_empty(), "empty reference schedule");
                assert_eq!(actual, *values, "{fixture} GT {gt}");
                assert!(block_ticks.collect_due(gt, 10, |_| true).is_empty());
                observed = true;
            }
            _ => {}
        }
    }
    assert!(observed, "missing ORDER-CHUNK fixture");
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_normal_direct_and_conducted_power_in_original_java() {
    let mut cells = BTreeMap::new();
    let mut observed = 0;
    for record in load_reference() {
        match record {
            Reference::Input {
                fixture,
                pos: [x, y, z],
                command: Some(command),
                ..
            } if is_wire_power_fixture(&fixture) => {
                let (_, description) = command
                    .split_once("minecraft:")
                    .expect("expected a test block");
                let (kind, properties) = description.split_once('[').unwrap_or((description, ""));
                cells.insert(BlockPos::new(x, y, z), signal_cell(kind, properties));
            }
            Reference::Sample {
                fixture,
                pos: [x, y, z],
                state: Some(state),
                ..
            } if is_wire_power_fixture(&fixture) => {
                let (kind, properties) = state
                    .strip_prefix("Block{minecraft:")
                    .and_then(|state| state.split_once('}'))
                    .expect("expected a sampled block state");
                cells.insert(
                    BlockPos::new(x, y, z),
                    signal_cell(kind, properties.trim_start_matches('[')),
                );
            }
            Reference::PowerQuery {
                fixture,
                gt,
                pos: [x, y, z],
                toward,
                signal,
            } if is_wire_power_fixture(&fixture) => {
                let position = BlockPos::new(x, y, z);
                let face = face(&toward);
                let cell = |pos| Some(cells.get(&pos).copied().unwrap_or_default());
                let output = power_output(position, face, PowerContext::Normal, cell).unwrap();
                assert_eq!(
                    ReferenceSignal {
                        normal: output.normal.get(),
                        direct: output.direct.get(),
                        emitted: output.emitted().unwrap().get(),
                    },
                    signal,
                    "{fixture} GT {gt} at {position:?} toward {face:?}",
                );
                observed += 1;
            }
            _ => {}
        }
    }
    assert_eq!(observed, 48, "incomplete six-face power fixtures");
}

fn is_wire_power_fixture(fixture: &str) -> bool {
    fixture.starts_with("PWR-") && fixture != "PWR-PISTON-GEOMETRY"
}

fn face(direction: &str) -> Face {
    match direction {
        "west" => Face::West,
        "east" => Face::East,
        "down" => Face::Down,
        "up" => Face::Up,
        "north" => Face::North,
        "south" => Face::South,
        _ => panic!("unexpected face {direction:?}"),
    }
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_original_piston_activation_geometry_for_all_six_faces() {
    let mut observed = 0;
    for record in load_reference() {
        if let Reference::PistonPowerQuery {
            gt,
            facing,
            offset: [x, y, z],
            powered,
        } = record
        {
            let source = BlockPos::new(x, y, z);
            let power = piston_power(BlockPos::new(0, 0, 0), face(&facing), |position| {
                Some(SignalCell {
                    conductor: false,
                    emitter: if position == source {
                        Emitter::RedstoneBlock
                    } else {
                        Emitter::None
                    },
                })
            });
            assert_eq!(
                matches!(power, PistonPower::Direct(_) | PistonPower::Quasi(_)),
                powered,
                "GT {gt} facing {facing} with source {source:?} returned {power:?}",
            );
            observed += 1;
        }
    }
    assert_eq!(observed, 84, "incomplete piston power probe");
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn plays_bud_inputs_through_the_authority_and_matches_original_ticks_and_queue() {
    assert_eq!(replay_piston_fixtures("BUD-"), (90, 30, 0));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_sticky_piston_pulses_and_quasi_power_across_the_chunk_seam() {
    assert_eq!(replay_piston_fixtures("TIM-STICKY-"), (151, 14, 0));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_bud007_update_propagation_range() {
    assert_eq!(replay_piston_fixtures("BUD007R-"), (9, 18, 0));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_bud007_update_propagation_across_directions() {
    assert_eq!(replay_piston_fixtures("BUD007D-"), (12, 24, 0));
    assert_eq!(replay_piston_fixtures("BUD007S-"), (1, 2, 0));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_bud008_shape_and_neighbor_updates_from_all_faces() {
    assert_eq!(replay_piston_fixtures("BUD008-FACE-"), (12, 12, 0));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_bud008_repeated_single_source_notifications() {
    assert_eq!(replay_piston_fixtures("BUD008-REPEAT-A"), (2, 4, 0));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn replays_bud008_neighbor_exclusion_inputs() {
    assert_eq!(replay_piston_fixtures("BUD008-EXCLUDE-"), (0, 0, 0));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_bud007_dust_cascade_and_piston_probe_order() {
    assert_eq!(replay_piston_fixtures("BUD007L-"), (26, 3, 0));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_bud007_extended_dust_cascades() {
    assert_eq!(replay_piston_fixtures("BUD007T-long"), (36, 3, 0));
    assert_eq!(replay_piston_fixtures("BUD007T-multi"), (36, 3, 0));
    assert_eq!(replay_piston_fixtures("BUD007T-corner"), (25, 3, 0));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_bud007_junction_and_section_cascades() {
    assert_eq!(replay_piston_fixtures("BUD007T-junction"), (41, 3, 0));
    assert_eq!(replay_piston_fixtures("BUD007T-section"), (18, 3, 0));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_bud007_update_propagation_across_components() {
    assert_eq!(replay_piston_fixtures("BUD007C-STICKY-"), (3, 6, 0));
    assert_eq!(
        replay_scheduled_block_fixtures("BUD007C-DISPENSER-"),
        (3, 6, 0)
    );
    assert_eq!(
        replay_scheduled_block_fixtures("BUD007C-DROPPER-"),
        (3, 6, 0)
    );
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_sticky_piston_shape_and_same_tick_bud_cases() {
    assert_eq!(replay_piston_fixtures("BUDX-STICKY-"), (11, 7, 0));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_dispenser_shape_and_same_tick_bud_cases() {
    assert_eq!(
        replay_scheduled_block_fixtures("BUDX-DISPENSER-"),
        (14, 14, 0)
    );
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_dropper_shape_and_same_tick_bud_cases() {
    assert_eq!(
        replay_scheduled_block_fixtures("BUDX-DROPPER-"),
        (14, 14, 0)
    );
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_quasi_power_piston_states_events_and_motion_after_a_process_restart() {
    assert_eq!(replay_piston_fixtures("RESTART-PISTON"), (356, 152, 51));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_resident_inactive_chunk_piston_events_motion_and_unnotified_power() {
    assert_eq!(replay_piston_fixtures("CHUNK-PISTON"), (168, 56, 0));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_piston_events_while_only_one_of_two_chunks_is_inactive() {
    assert_eq!(replay_piston_fixtures("CHUNK-PARTIAL"), (6, 6, 0));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_overdue_block_ticks_when_a_resident_chunk_starts_ticking_again() {
    assert_eq!(replay_scheduled_block_fixtures("CHUNK-TIM"), (26, 26, 0));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn retains_dispenser_and_dropper_callbacks_when_one_chunk_is_inactive() {
    assert_eq!(
        replay_scheduled_block_fixtures("CHUNK-PARTIAL-EJECTOR"),
        (6, 6, 0)
    );
}

fn replay_piston_fixtures(prefix: &str) -> (usize, usize, usize) {
    let preset = RulePreset::JavaFlatV3;
    let mut world = Server::playable_with_rules(0, preset.default_range(), preset).unwrap();
    let mut session = world.open_local_session(Capabilities::P1_HOST);
    let mut operator = world.open_local_operator().unwrap();
    let mut operation_id = 0;
    let mut samples = 0;
    let mut queue_samples = 0;
    let mut progress_samples = 0;
    let mut gt_offset = 0;
    for record in load_reference() {
        match record {
            Reference::ChunkActivity {
                fixture,
                gt,
                chunk: [x, z],
                active,
                resident,
            } if fixture.starts_with(prefix) => {
                advance_to(&mut world, gt + gt_offset);
                operation_id += 1;
                let position = ChunkPos { x, z };
                submit_fixture(
                    &mut world,
                    session,
                    operation_id,
                    Mutation::SetChunkActive { position, active },
                );
                let checkpoint = world.checkpoint().unwrap();
                let saved = checkpoint
                    .chunks
                    .iter()
                    .find(|chunk| chunk.position == position);
                assert_eq!(saved.is_some(), resident, "{fixture} GT {gt} resident");
                assert_eq!(saved.unwrap().active, active, "{fixture} GT {gt} active");
                if fixture.starts_with("CHUNK-PARTIAL-EJECTOR") {
                    eprintln!(
                        "{fixture}: changed {position:?} to {active}; checkpoint chunks={:?}",
                        world
                            .checkpoint()
                            .unwrap()
                            .chunks
                            .iter()
                            .map(|chunk| (chunk.position, chunk.active))
                            .collect::<Vec<_>>()
                    );
                }
            }
            Reference::WorldReloaded { fixture, gt } if fixture.starts_with(prefix) => {
                world = Server::restore(world.checkpoint().unwrap()).unwrap();
                session = world.open_local_session(Capabilities::P1_HOST);
                operator = world.open_local_operator().unwrap();
                gt_offset = world
                    .snapshot()
                    .tick
                    .checked_sub(gt)
                    .expect("reopened reference clock");
            }
            Reference::Input {
                fixture,
                gt,
                pos: [x, y, z],
                operation: Some(operation),
                ..
            } if fixture.starts_with(prefix) && operation == "place_wire" => {
                advance_to(&mut world, gt + gt_offset);
                let position = BlockPos::new(x, y, z);
                operation_id += 1;
                apply_fixture_wire_action(&mut world, operator, operation_id * 4, position, true);
            }
            Reference::Input {
                fixture,
                gt,
                pos: [x, y, z],
                command,
                operation,
            } if fixture.starts_with(prefix) => {
                advance_to(&mut world, gt + gt_offset);
                let position = BlockPos::new(x, y, z);
                let mutation = if let Some(command) = command {
                    let block = command.split_once("minecraft:").unwrap().1;
                    let snapshot = world.snapshot();
                    Mutation::SetBlock {
                        position,
                        state: world.registry().block_id(&reference_block(block)).unwrap(),
                        expected_revision: snapshot.chunks[&position.chunk()].revision(),
                    }
                } else {
                    let name = operation.as_deref().expect("reference block notification");
                    if let Some(direction) = name.strip_prefix("shape_update_from_") {
                        Mutation::NotifyJavaBlock {
                            target: position,
                            from: face(direction),
                            kind: JavaNotification::ShapeUpdate,
                        }
                    } else if let Some(direction) = name.strip_prefix("update_neighbors_except_") {
                        Mutation::NotifyJavaNeighborsExcept {
                            source: position,
                            excluded: face(direction),
                        }
                    } else {
                        let coords: Vec<i64> = name
                            .strip_prefix("neighbor_changed_from_")
                            .unwrap()
                            .split('_')
                            .map(|n| n.parse().unwrap())
                            .collect();
                        let source = BlockPos::new(coords[0], coords[1], coords[2]);
                        let from = [
                            Face::West,
                            Face::East,
                            Face::Down,
                            Face::Up,
                            Face::North,
                            Face::South,
                        ]
                        .into_iter()
                        .find(|side| side.offset(position) == Some(source))
                        .expect("reference source next to target");
                        Mutation::NotifyJavaBlock {
                            target: position,
                            from,
                            kind: JavaNotification::NeighborChanged,
                        }
                    }
                };
                operation_id += 1;
                let report = world.step_local(
                    vec![Request {
                        session,
                        operation: operation_id,
                        mutation,
                    }],
                    false,
                    false,
                );
                assert!(
                    report.acks[0].result.is_ok(),
                    "{fixture} GT {gt}: {:?}",
                    report.acks
                );
            }
            Reference::Sample {
                fixture,
                gt,
                pos: [x, y, z],
                state: Some(state),
            } if fixture.starts_with(prefix) => {
                advance_to(&mut world, gt + gt_offset);
                let position = BlockPos::new(x, y, z);
                let actual = fixture_block_name(&world, position);
                let (kind, properties) = state
                    .strip_prefix("Block{minecraft:")
                    .unwrap()
                    .split_once('}')
                    .unwrap();
                let state = format!("{kind}{properties}");
                assert_eq!(
                    actual,
                    reference_block(&state),
                    "{fixture} GT {gt} sampled {position:?}"
                );
                samples += 1;
            }
            Reference::BlockEventQueue {
                fixture,
                gt,
                pos: [x, y, z],
                pending,
            } if fixture.starts_with(prefix) => {
                advance_to(&mut world, gt + gt_offset);
                let position = BlockPos::new(x, y, z);
                let actual: Vec<_> = world
                    .pending_piston_events()
                    .filter(|event| event.base == position)
                    .map(|event| ReferencePistonEvent {
                        block: match event.kind {
                            PistonKind::Normal => "Block{minecraft:piston}",
                            PistonKind::Sticky => "Block{minecraft:sticky_piston}",
                        }
                        .into(),
                        id: event.action.java_event_id(),
                        data: [3, 5, 2, 4, 0, 1][usize::from(event.facing)],
                    })
                    .collect();
                assert_eq!(actual, pending, "{fixture} GT {gt} queue for {position:?}");
                queue_samples += 1;
            }
            Reference::BlockEventOrder {
                fixture,
                gt,
                phase,
                probes,
                pending,
            } if fixture.starts_with(prefix) => {
                advance_to(&mut world, gt + gt_offset);
                let probes: BTreeSet<_> = probes
                    .into_iter()
                    .map(|[x, y, z]| BlockPos::new(x, y, z))
                    .collect();
                let actual: Vec<_> = world
                    .pending_piston_events()
                    .filter(|event| probes.contains(&event.base))
                    .map(|event| ReferencePistonEventAt {
                        pos: [event.base.x, event.base.y, event.base.z],
                        block: match event.kind {
                            PistonKind::Normal => "Block{minecraft:piston}",
                            PistonKind::Sticky => "Block{minecraft:sticky_piston}",
                        }
                        .into(),
                        id: event.action.java_event_id(),
                        data: [3, 5, 2, 4, 0, 1][usize::from(event.facing)],
                    })
                    .collect();
                assert_eq!(
                    actual, pending,
                    "{fixture} GT {gt} phase {phase} block event order"
                );
                queue_samples += 1;
            }
            Reference::PistonProgress {
                fixture,
                gt,
                pos: [x, y, z],
                base: [bx, by, bz],
                previous,
                current,
                block_ticking,
                entities_loaded,
            } if fixture.starts_with(prefix) => {
                advance_to(&mut world, gt + gt_offset);
                let position = BlockPos::new(x, y, z);
                let motion = world
                    .inspect_java_bud(BlockPos::new(bx, by, bz))
                    .motion
                    .into_iter()
                    .find(|motion| motion.position == position)
                    .unwrap_or_else(|| panic!("{fixture} GT {gt} no motion at {position:?}"));
                let progress = |remaining| f32::from(3 - remaining) * 0.5;
                assert_eq!(
                    (
                        progress(motion.previous_remaining),
                        progress(motion.remaining)
                    ),
                    (previous, current),
                    "{fixture} GT {gt} motion at {position:?} (block_ticking={block_ticking}, entities_loaded={entities_loaded})",
                );
                progress_samples += 1;
            }
            _ => {}
        }
    }
    (samples, queue_samples, progress_samples)
}

// Observe inactive resident state through a read-only checkpoint. The client
// snapshot intentionally omits such chunks; observing must not reactivate them.
fn fixture_block_name(world: &Server, position: BlockPos) -> String {
    if let Some(chunk) = world.snapshot().chunks.get(&position.chunk()) {
        return world
            .registry()
            .block(chunk.block(position).unwrap())
            .unwrap()
            .name
            .into();
    }
    let checkpoint = world.checkpoint().unwrap();
    let chunk = checkpoint
        .chunks
        .iter()
        .find(|chunk| chunk.position == position.chunk())
        .unwrap();
    let Some(section) = chunk
        .sections
        .iter()
        .find(|section| section.y == position.y.div_euclid(16))
    else {
        return "rustcraft:empty".into();
    };
    let index = ((position.y.rem_euclid(16) * 16 + position.z.rem_euclid(16)) * 16
        + position.x.rem_euclid(16)) as usize;
    let palette = section.indices.get(index).copied().unwrap_or(0) as usize;
    section.palette[palette].name.clone()
}

fn advance_to(world: &mut Server, gt: u64) {
    let mut current = world.snapshot().tick;
    assert!(current <= gt, "original trace out of order");
    while current < gt {
        current = world.step(Vec::new()).snapshot.tick;
    }
}

fn reference_block(state: &str) -> String {
    let kind = state.split('[').next().unwrap();
    match kind {
        "air" => "rustcraft:empty".into(),
        "redstone_block" => "rustcraft_java:redstone_block".into(),
        "glass" => "rustcraft_java:glass".into(),
        "smooth_stone" => "rustcraft_java:smooth_stone".into(),
        "redstone_lamp" if state.contains("lit=true") => "rustcraft_java:redstone_lamp_on".into(),
        "redstone_lamp" => "rustcraft_java:redstone_lamp".into(),
        "dispenser" | "dropper" => {
            let facing = state.split_once("facing=").map_or("north", |(_, value)| {
                value.split([',', ']']).next().unwrap()
            });
            let facing = if facing == "north" {
                String::new()
            } else {
                format!("_{facing}")
            };
            let trigger = if state.contains("triggered=true") {
                "_triggered"
            } else {
                ""
            };
            format!("rustcraft_java:{kind}{facing}{trigger}")
        }
        "lever" | "stone_button" | "oak_button" => {
            let property = |key: &str, default: &'static str| {
                state.split_once(key).map_or(default, |(_, value)| {
                    value.split([',', ']']).next().unwrap()
                })
            };
            let mount = property("face=", "wall");
            let facing = property("facing=", "north");
            let canonical = if kind == "lever" {
                mount == "floor" && facing == "south"
            } else {
                mount == "wall" && facing == "north"
            };
            let suffix = if canonical {
                String::new()
            } else {
                format!("_{mount}_{facing}")
            };
            let powered = if state.contains("powered=true") {
                "_on"
            } else {
                ""
            };
            format!("rustcraft_java:{kind}{suffix}{powered}")
        }
        "stone_pressure_plate" | "oak_pressure_plate" => {
            let powered = if state.contains("powered=true") {
                "_on"
            } else {
                ""
            };
            format!("rustcraft_java:{kind}{powered}")
        }
        "light_weighted_pressure_plate" | "heavy_weighted_pressure_plate" => {
            let power = state
                .split_once("power=")
                .map_or("0", |(_, value)| value.split([',', ']']).next().unwrap());
            let strength = if power == "0" {
                String::new()
            } else {
                format!("_power_{power}")
            };
            format!("rustcraft_java:{kind}{strength}")
        }
        "cake" => {
            let bites = state
                .split_once("bites=")
                .map_or("0", |(_, bites)| bites.split([',', ']']).next().unwrap());
            if bites == "0" {
                "rustcraft_java:cake".into()
            } else {
                format!("rustcraft_java:cake_bites_{bites}")
            }
        }
        "redstone_torch" | "redstone_wall_torch" => {
            let facing = state
                .split_once("facing=")
                .map_or(String::new(), |(_, facing)| {
                    format!("_{}", facing.split([',', ']']).next().unwrap())
                });
            let lit = if state.contains("lit=false") {
                "_off"
            } else {
                ""
            };
            format!("rustcraft_java:{kind}{facing}{lit}")
        }
        "redstone_wire" => {
            let property = |key: &str, default: &'static str| {
                state.split_once(key).map_or(default, |(_, value)| {
                    value.split([',', ']']).next().unwrap()
                })
            };
            let power = property("power=", "0");
            let index: usize = ["south=", "east=", "north=", "west="]
                .into_iter()
                .zip([1, 3, 9, 27])
                .map(|(face, factor)| {
                    factor
                        * match property(face, "none") {
                            "none" => 0,
                            "side" => 1,
                            "up" => 2,
                            other => panic!("invalid wire connection {other}"),
                        }
                })
                .sum();
            format!("rustcraft_java:wire_{power}_{index}")
        }
        "observer" => {
            let direction = state
                .split_once("facing=")
                .unwrap()
                .1
                .split([',', ']'])
                .next()
                .unwrap();
            let facing = if direction == "south" {
                String::new()
            } else {
                format!("_{direction}")
            };
            let power = if state.contains("powered=true") {
                "_on"
            } else {
                ""
            };
            format!("rustcraft_java:observer{facing}{power}")
        }
        "repeater" => {
            let output = diode_facing_suffix(state);
            let delay = state
                .split_once("delay=")
                .map_or("1", |(_, value)| value.split([',', ']']).next().unwrap());
            let delay = if delay == "1" {
                String::new()
            } else {
                format!("_delay{delay}")
            };
            let lock = if state.contains("locked=true") {
                "_locked"
            } else {
                ""
            };
            let power = if state.contains("powered=true") {
                "_on"
            } else {
                ""
            };
            format!("rustcraft_java:repeater{output}{delay}{lock}{power}")
        }
        "comparator" => {
            let output = diode_facing_suffix(state);
            let mode = if state.contains("mode=subtract") {
                "_subtract"
            } else {
                ""
            };
            let power = if state.contains("powered=true") {
                "_on"
            } else {
                ""
            };
            format!("rustcraft_java:comparator{output}{mode}{power}")
        }
        "piston" | "sticky_piston" | "piston_head" | "moving_piston" => {
            let direction = state
                .split_once("facing=")
                .unwrap()
                .1
                .split([',', ']'])
                .next()
                .unwrap();
            let base = matches!(kind, "piston" | "sticky_piston");
            let facing = if base && direction == "east" {
                String::new()
            } else {
                format!("_{direction}")
            };
            let extended = if base && state.contains("extended=true") {
                "_extended"
            } else {
                ""
            };
            let rustcraft_kind = match (kind, state.contains("type=sticky")) {
                ("moving_piston", true) => "moving_sticky_piston",
                ("piston_head", true) => "sticky_piston_head",
                _ => kind,
            };
            format!("rustcraft_java:{rustcraft_kind}{facing}{extended}")
        }
        _ => panic!("unhandled reference state {state:?}"),
    }
}

fn diode_facing_suffix(state: &str) -> &'static str {
    let input = state.split_once("facing=").map_or("north", |(_, facing)| {
        facing.split([',', ']']).next().unwrap()
    });
    match input {
        "west" => "",
        "east" => "_west",
        "north" => "_south",
        "south" => "_north",
        _ => panic!("non-horizontal diode facing {input}"),
    }
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_lamp_delay_and_repowering_with_the_live_scheduled_block_queue() {
    assert_eq!(replay_scheduled_block_fixtures("TIM-LAMP"), (61, 42, 0));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_repeater_delays_priority_pulse_and_lock() {
    assert_eq!(
        replay_scheduled_block_fixtures("TIM-REPEATER"),
        (141, 98, 0)
    );
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_comparator_modes_analog_input_scheduling_and_repeater_lock() {
    assert_eq!(
        replay_scheduled_block_fixtures("TIM-COMPARATOR"),
        (171, 171, 40)
    );
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_comparator_block_entity_output_and_pending_tick_after_reopen() {
    assert_eq!(
        replay_scheduled_block_fixtures("RESTART-COMPARATOR"),
        (4, 4, 8)
    );
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_dust_power_from_vertical_and_conducted_sources() {
    assert_eq!(
        replay_scheduled_block_fixtures("TIM-WIRE-EXTERNAL"),
        (21, 8, 1)
    );
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_dust_power_notices_and_their_effect_on_lamp_queue_order() {
    assert_eq!(
        replay_scheduled_block_fixtures("TIM-WIRE-NOTIFY"),
        (167, 171, 0)
    );
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_wire_dots_crosses_corners_and_high_low_connections() {
    assert_eq!(replay_scheduled_block_fixtures("SHAPE-WIRE"), (41, 0, 180));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_button_click_deadlines_and_attachment_power() {
    assert_eq!(replay_scheduled_block_fixtures("TIM-BUTTON"), (37, 35, 18));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_lever_placement_power_and_removal_on_each_mount() {
    assert_eq!(replay_scheduled_block_fixtures("TIM-LEVER"), (113, 98, 37));
    let mut placements = 0;
    for record in load_reference() {
        let Reference::LeverPlacement { phase, state } = record else {
            continue;
        };
        let preset = RulePreset::JavaFlatV2;
        let mut world = Server::playable_with_rules(0, preset.default_range(), preset).unwrap();
        let host = world.open_local_session(Capabilities::P1_HOST);
        let operator = world.open_local_operator().unwrap();
        assert_eq!(
            place_fixture_block(
                &mut world,
                host,
                operator,
                1,
                BlockPos::new(10, 6, 7),
                face(&phase),
                "minecraft:lever",
            ),
            reference_sample_block(&state),
            "lever attached toward {phase}",
        );
        placements += 1;
    }
    assert_eq!(placements, 6);
}

#[test]
fn removing_a_ceiling_lever_support_schedules_the_direct_lamp_first() {
    let preset = RulePreset::JavaFlatV2;
    let mut world = Server::playable_with_rules(0, preset.default_range(), preset).unwrap();
    let host = world.open_local_session(Capabilities::P1_HOST);
    let operator = world.open_local_operator().unwrap();
    let lever = BlockPos::new(9, 50, 9);
    let support = Face::Up.offset(lever).unwrap();
    let direct = Face::East.offset(lever).unwrap();
    let conducted = Face::North.offset(support).unwrap();
    for (index, (position, name)) in [
        (support, "rustcraft_java:smooth_stone"),
        (direct, "rustcraft_java:redstone_lamp"),
        (conducted, "rustcraft_java:redstone_lamp"),
        (lever, "rustcraft_java:lever_ceiling_west"),
    ]
    .into_iter()
    .enumerate()
    {
        let state = world.registry().block_id(name).unwrap();
        let expected_revision = world.snapshot().chunks[&position.chunk()].revision();
        submit_fixture(
            &mut world,
            host,
            index as u64 + 1,
            Mutation::SetBlock {
                position,
                state,
                expected_revision,
            },
        );
    }
    use_fixture_control(&mut world, operator, 1, lever, true);
    let expected_revision = world.snapshot().chunks[&support.chunk()].revision();
    let state = world.registry().block_id("rustcraft:empty").unwrap();
    submit_fixture(
        &mut world,
        host,
        5,
        Mutation::SetBlock {
            position: support,
            state,
            expected_revision,
        },
    );
    let queue: Vec<_> = world
        .pending_java_block_ticks()
        .map(|tick| (tick.position, tick.due_gt))
        .collect();
    assert_eq!(queue, [(direct, 4), (conducted, 4)]);
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_dispenser_and_dropper_quasi_power_inventory_and_ejection_tick() {
    assert_eq!(replay_scheduled_block_fixtures("TIM-EJECTOR"), (67, 65, 4));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_ejector_quasi_power_and_scheduled_ticks_after_a_process_restart() {
    assert_eq!(
        replay_scheduled_block_fixtures("RESTART-EJECTOR"),
        (34, 34, 0)
    );
}

#[test]
fn an_ejected_item_presses_a_weighted_plate_and_leaves_stone_unpowered() {
    for (name, power) in [
        ("minecraft:light_weighted_pressure_plate", 1),
        ("minecraft:stone_pressure_plate", 0),
    ] {
        let preset = RulePreset::JavaFlatV2;
        let mut world = Server::playable_with_rules(0, preset.default_range(), preset).unwrap();
        let host = world.open_local_session(Capabilities::P1_HOST);
        let plate = BlockPos::new(7, 10, 6);
        let ejector = Face::Up.offset(plate).unwrap();
        let mut sequence = 0;
        for (position, block) in [
            (Face::Down.offset(plate).unwrap(), "minecraft:smooth_stone"),
            (Face::East.offset(plate).unwrap(), "minecraft:glass"),
            (Face::West.offset(plate).unwrap(), "minecraft:glass"),
            (Face::North.offset(plate).unwrap(), "minecraft:glass"),
            (Face::South.offset(plate).unwrap(), "minecraft:glass"),
            (plate, name),
            (ejector, "minecraft:dropper[facing=down]"),
        ] {
            sequence += 1;
            let block = world
                .registry()
                .block_id(&reference_block(block.strip_prefix("minecraft:").unwrap()))
                .unwrap();
            let expected_revision = world.snapshot().chunks[&position.chunk()].revision();
            submit_fixture(
                &mut world,
                host,
                sequence,
                Mutation::SetBlock {
                    position,
                    state: block,
                    expected_revision,
                },
            );
        }
        sequence += 1;
        load_fixture_ejector(
            &mut world,
            host,
            sequence,
            ejector,
            "{Items:[{Slot:4b,id:\"minecraft:smooth_stone\",Count:1b}]}",
        );
        sequence += 1;
        let position = Face::East.offset(ejector).unwrap();
        let state = world
            .registry()
            .block_id("rustcraft_java:redstone_block")
            .unwrap();
        let expected_revision = world.snapshot().chunks[&position.chunk()].revision();
        submit_fixture(
            &mut world,
            host,
            sequence,
            Mutation::SetBlock {
                position,
                state,
                expected_revision,
            },
        );
        for _ in 0..28 {
            world.step(Vec::new());
        }
        let snapshot = world.snapshot();
        assert_eq!(snapshot.dropped_items.len(), 1);
        assert!((snapshot.dropped_items[0].1.position[1] - plate.y as f64).abs() < 0.01);
        assert_eq!(
            world
                .java_power_output(plate, Face::Down)
                .unwrap()
                .direct
                .get(),
            power
        );
    }
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_pressure_plate_entry_hold_and_release_with_one_occupant() {
    assert_eq!(replay_scheduled_block_fixtures("TIM-PLATE"), (36, 36, 24));
}

#[test]
fn places_a_button_on_the_clicked_floor_wall_or_ceiling() {
    for (support, name) in [
        (Face::Down, "rustcraft_java:stone_button_floor_south"),
        (Face::West, "rustcraft_java:stone_button_wall_east"),
        (Face::Up, "rustcraft_java:stone_button_ceiling_south"),
    ] {
        let preset = RulePreset::JavaFlatV2;
        let mut world = Server::playable_with_rules(0, preset.default_range(), preset).unwrap();
        let host = world.open_local_session(Capabilities::P1_HOST);
        let operator = world.open_local_operator().unwrap();
        assert_eq!(
            place_fixture_block(
                &mut world,
                host,
                operator,
                1,
                BlockPos::new(7, 8, 6),
                support,
                "minecraft:stone_button"
            ),
            name
        );
    }
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_observer_pulses_and_directional_power() {
    assert_eq!(
        replay_scheduled_block_fixtures("TIM-OBSERVER"),
        (163, 170, 12)
    );
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_torch_input_output_and_burnout_windows() {
    assert_eq!(replay_scheduled_block_fixtures("TIM-TORCH"), (186, 187, 60));
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_original_restore_of_torch_recovery_and_loss_of_toggle_history() {
    assert_eq!(
        replay_scheduled_block_fixtures("RESTART-TORCH"),
        (16, 13, 0)
    );
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn matches_original_torch_attachment_when_the_crosshair_targets_the_wall() {
    let mut count = 0;
    for record in load_reference() {
        if let Reference::TorchAttachment {
            phase,
            pos: [x, y, z],
            state,
        } = record
        {
            assert_eq!(phase, "look_down_click_wall");
            assert_eq!(
                place_torch_while_looking_down(BlockPos::new(x, y, z)),
                reference_sample_block(&state)
            );
            count += 1;
        }
    }
    assert_eq!(count, 1);
}

#[test]
#[ignore = "requires an original Java 1.20.1 reference trace"]
fn places_observers_with_the_same_six_orientations_as_original_java() {
    let mut count = 0;
    for record in load_reference() {
        if let Reference::ObserverPlacement { phase, state } = record {
            let preset = RulePreset::JavaFlatV2;
            let mut world = Server::playable_with_rules(0, preset.default_range(), preset).unwrap();
            let host = world.open_local_session(Capabilities::P1_HOST);
            let operator = world.open_local_operator().unwrap();
            let position = BlockPos::new(7, 8, 6);
            let actual = place_fixture_block(
                &mut world,
                host,
                operator,
                1,
                position,
                face(&phase),
                "minecraft:observer",
            );
            assert_eq!(actual, reference_sample_block(&state), "look {phase}");
            assert_eq!(world.pending_java_block_ticks().count(), 0);
            count += 1;
        }
    }
    assert_eq!(count, 6);
}

#[test]
fn installs_the_redstone_torch_item_on_the_available_floor_or_wall() {
    for (look, state) in [
        (Face::Down, "rustcraft_java:redstone_torch"),
        (Face::West, "rustcraft_java:redstone_wall_torch_east"),
    ] {
        let preset = RulePreset::JavaFlatV2;
        let mut world = Server::playable_with_rules(0, preset.default_range(), preset).unwrap();
        let host = world.open_local_session(Capabilities::P1_HOST);
        let operator = world.open_local_operator().unwrap();
        let position = BlockPos::new(7, 8, 6);
        let actual = place_fixture_block(
            &mut world,
            host,
            operator,
            1,
            position,
            look,
            "minecraft:redstone_torch",
        );
        assert_eq!(actual, state);
        assert_eq!(world.pending_java_block_ticks().count(), 0);
    }
    assert_eq!(
        place_torch_while_looking_down(BlockPos::new(7, 8, 6)),
        "rustcraft_java:redstone_wall_torch_east"
    );
}

#[test]
fn player_placement_initializes_a_lamp_from_its_live_input() {
    let preset = RulePreset::JavaFlatV2;
    let mut world = Server::playable_with_rules(0, preset.default_range(), preset).unwrap();
    let host = world.open_local_session(Capabilities::P1_HOST);
    let operator = world.open_local_operator().unwrap();
    let target = BlockPos::new(7, 8, 6);
    let position = Face::East.offset(target).unwrap();
    let expected_revision = world.snapshot().chunks[&position.chunk()].revision();
    let state = world
        .registry()
        .block_id("rustcraft_java:redstone_block")
        .unwrap();
    submit_fixture(
        &mut world,
        host,
        1,
        Mutation::SetBlock {
            position,
            state,
            expected_revision,
        },
    );
    assert_eq!(
        place_fixture_block(
            &mut world,
            host,
            operator,
            2,
            target,
            Face::Down,
            "minecraft:redstone_lamp"
        ),
        "rustcraft_java:redstone_lamp_on"
    );
}

fn place_torch_while_looking_down(position: BlockPos) -> String {
    let preset = RulePreset::JavaFlatV2;
    let mut world = Server::playable_with_rules(0, preset.default_range(), preset).unwrap();
    let host = world.open_local_session(Capabilities::P1_HOST);
    let operator = world.open_local_operator().unwrap();
    let wall = Face::West.offset(position).unwrap();
    let floor = Face::Down.offset(position).unwrap();
    let state = world
        .registry()
        .block_id("rustcraft_java:smooth_stone")
        .unwrap();
    for (index, support) in [wall, floor].into_iter().enumerate() {
        let revision = world.snapshot().chunks[&support.chunk()].revision();
        submit_fixture(
            &mut world,
            host,
            index as u64 + 1,
            Mutation::SetBlock {
                position: support,
                state,
                expected_revision: revision,
            },
        );
    }
    submit_fixture(
        &mut world,
        operator,
        1,
        Mutation::PlayerCommand(PlayerCommand::Give {
            item: "minecraft:redstone_torch".into(),
            count: 1,
        }),
    );
    let feet = [
        position.x as f64 + 0.6,
        position.y as f64 + 0.28,
        position.z as f64 + 0.5,
    ];
    let aimed = submit_fixture(
        &mut world,
        operator,
        2,
        Mutation::PlayerCommand(PlayerCommand::TeleportFacing {
            position: feet.map(Coordinate::Absolute),
            rotation: [90.0, 65.0].map(Coordinate::Absolute),
        }),
    );
    let target = aimed.snapshot.player.unwrap().target.unwrap();
    assert_eq!(target.position, wall);
    assert_eq!(target.placement.unwrap().position, position);
    let report = submit_fixture(
        &mut world,
        operator,
        3,
        Mutation::UseBlock {
            action: BlockAction::Place,
            target,
        },
    );
    let block = report.snapshot.chunks[&position.chunk()]
        .block(position)
        .unwrap();
    world.registry().block(block).unwrap().name.into()
}

#[test]
fn changes_the_repeater_setting_without_replacing_an_already_due_callback() {
    let preset = RulePreset::JavaFlatV2;
    let mut world = Server::playable_with_rules(0, preset.default_range(), preset).unwrap();
    let host = world.open_local_session(Capabilities::P1_HOST);
    let operator = world.open_local_operator().unwrap();
    let repeater = BlockPos::new(7, 8, 6);
    for (operation, position, name) in [
        (1, BlockPos::new(7, 7, 6), "rustcraft_java:smooth_stone"),
        (2, BlockPos::new(6, 8, 6), "rustcraft_java:redstone_block"),
    ] {
        let expected_revision = world.snapshot().chunks[&position.chunk()].revision();
        let state = world.registry().block_id(name).unwrap();
        let report = world.step_local(
            vec![Request {
                session: host,
                operation,
                mutation: Mutation::SetBlock {
                    position,
                    state,
                    expected_revision,
                },
            }],
            false,
            false,
        );
        assert!(report.acks[0].result.is_ok());
    }
    place_east_facing_diode(&mut world, operator, 1, repeater, "minecraft:repeater");
    let pending = world.pending_java_block_ticks().next().unwrap();
    for (index, suffix) in ["_delay2", "_delay3", "_delay4", ""]
        .into_iter()
        .enumerate()
    {
        let target = world.snapshot().player.unwrap().target.unwrap();
        assert_eq!(target.position, repeater);
        let report = world.step_local(
            vec![Request {
                session: operator,
                operation: index as u64 + 4,
                mutation: Mutation::UseBlock {
                    action: BlockAction::Place,
                    target,
                },
            }],
            false,
            false,
        );
        assert!(report.acks[0].result.is_ok(), "{:?}", report.acks[0]);
        let state = report.snapshot.chunks[&repeater.chunk()]
            .block(repeater)
            .unwrap();
        assert_eq!(
            world.registry().block(state).unwrap().name,
            format!("rustcraft_java:repeater{suffix}")
        );
        assert_eq!(
            world.pending_java_block_ticks().collect::<Vec<_>>(),
            vec![pending]
        );
    }
}

fn replay_scheduled_block_fixtures(prefix: &str) -> (usize, usize, usize) {
    eprintln!("replaying scheduled prefix {prefix}");
    let preset = RulePreset::JavaFlatV3;
    let mut world = Server::playable_with_rules(0, preset.default_range(), preset).unwrap();
    let mut session = world.open_local_session(Capabilities::P1_HOST);
    let mut operator = world.open_local_operator().unwrap();
    let mut sequence = 0;
    let mut samples = 0;
    let mut queue_samples = 0;
    let mut power_samples = 0;
    let mut queue_orders = BTreeMap::<(u64, String), BTreeMap<i64, u64>>::new();
    let mut reload_epoch = 0;
    let mut gt_offset = 0;
    let mut inventory_samples = 0;
    let mut entity_samples = 0;
    for record in load_reference() {
        match record {
            Reference::ChunkActivity {
                fixture,
                gt,
                chunk: [x, z],
                active,
                resident,
            } => {
                if fixture.contains("PARTIAL") {
                    eprintln!("activity fixture={fixture:?} prefix={prefix:?}");
                }
                if !fixture.starts_with(prefix) {
                    continue;
                }
                eprintln!("scheduled replay {fixture}: chunk [{x},{z}] active={active}");
                advance_to(&mut world, gt + gt_offset);
                sequence += 1;
                let position = ChunkPos { x, z };
                submit_fixture(
                    &mut world,
                    session,
                    sequence,
                    Mutation::SetChunkActive { position, active },
                );
                let checkpoint = world.checkpoint().unwrap();
                let saved = checkpoint
                    .chunks
                    .iter()
                    .find(|chunk| chunk.position == position);
                assert_eq!(saved.is_some(), resident, "{fixture} GT {gt} resident");
                assert_eq!(saved.unwrap().active, active, "{fixture} GT {gt} active");
            }
            Reference::WorldReloaded { fixture, gt } if fixture.starts_with(prefix) => {
                reload_epoch += 1;
                world = Server::restore(world.checkpoint().unwrap()).unwrap();
                session = world.open_local_session(Capabilities::P1_HOST);
                operator = world.open_local_operator().unwrap();
                gt_offset = world
                    .snapshot()
                    .tick
                    .checked_sub(gt)
                    .expect("reopened reference clock");
            }
            Reference::Input {
                fixture,
                gt,
                pos: [x, y, z],
                command: Some(command),
                ..
            } if fixture.starts_with(prefix) => {
                eprintln!("input fixture={fixture:?} prefix={prefix:?}");
                advance_to(&mut world, gt + gt_offset);
                let position = BlockPos::new(x, y, z);
                if command.starts_with("data merge block ") {
                    sequence += 1;
                    load_fixture_ejector(&mut world, session, sequence, position, &command);
                    continue;
                }
                let block = command.split_once("minecraft:").unwrap().1;
                if (fixture == "TIM-REPEATER-PLACED-BY" && block.starts_with("repeater["))
                    || (fixture == "TIM-COMPARATOR-PLACED-BY" && block.starts_with("comparator["))
                {
                    // Replay the write and placement callback together via player use.
                    continue;
                }
                let checkpoint = world.checkpoint().unwrap();
                let chunk = checkpoint
                    .chunks
                    .iter()
                    .find(|chunk| chunk.position == position.chunk())
                    .unwrap_or_else(|| {
                        panic!(
                            "{fixture} GT {gt}: input chunk {:?} is not resident for {command}",
                            position.chunk()
                        )
                    });
                assert!(
                    chunk.active,
                    "{fixture} GT {gt}: input chunk {:?} is inactive",
                    position.chunk()
                );
                let revision = chunk.revision;
                let state = world.registry().block_id(&reference_block(block)).unwrap();
                sequence += 1;
                let report = world.step_local(
                    vec![Request {
                        session,
                        operation: sequence,
                        mutation: Mutation::SetBlock {
                            position,
                            state,
                            expected_revision: revision,
                        },
                    }],
                    false,
                    false,
                );
                assert!(
                    report.acks[0].result.is_ok(),
                    "{fixture} GT {gt}: {:?}",
                    report.acks
                );
                assert_eq!(
                    world.scheduled_tasks().len(),
                    0,
                    "{fixture} used the general task queue"
                );
            }
            Reference::Input {
                fixture,
                gt,
                pos: [x, y, z],
                operation: Some(operation),
                ..
            } if matches!(
                fixture.as_str(),
                "TIM-REPEATER-PLACED-BY" | "TIM-COMPARATOR-PLACED-BY"
            ) && fixture.starts_with(prefix) =>
            {
                advance_to(&mut world, gt + gt_offset);
                let item = match operation.as_str() {
                    "placed_by" => "minecraft:repeater",
                    "placed_comparator" => "minecraft:comparator",
                    _ => panic!("unexpected placement operation {operation}"),
                };
                sequence += 1;
                place_east_facing_diode(
                    &mut world,
                    operator,
                    4 * sequence,
                    BlockPos::new(x, y, z),
                    item,
                );
            }
            Reference::Input {
                fixture,
                gt,
                pos: [x, y, z],
                operation: Some(operation),
                ..
            } if fixture.starts_with(prefix) => {
                advance_to(&mut world, gt + gt_offset);
                let position = BlockPos::new(x, y, z);
                sequence += 1;
                if matches!(operation.as_str(), "place_wire" | "use_wire") {
                    apply_fixture_wire_action(
                        &mut world,
                        operator,
                        sequence * 4,
                        position,
                        operation == "place_wire",
                    );
                } else if operation == "place_observer_west" {
                    place_fixture_block(
                        &mut world,
                        session,
                        operator,
                        sequence,
                        position,
                        Face::West,
                        "minecraft:observer",
                    );
                } else if operation == "notify_comparator_output" {
                    submit_fixture(
                        &mut world,
                        session,
                        sequence,
                        Mutation::NotifyJavaComparatorOutput { source: position },
                    );
                } else if matches!(
                    operation.as_str(),
                    "use_comparator" | "use_button" | "use_lever"
                ) {
                    use_fixture_control(
                        &mut world,
                        operator,
                        sequence * 4,
                        position,
                        matches!(fixture.as_str(), "TIM-BUTTON-OAK" | "TIM-LEVER-ceiling"),
                    );
                } else if matches!(operation.as_str(), "occupy_plate" | "leave_plate") {
                    let feet = if operation == "occupy_plate" {
                        [x as f64 + 0.5, y as f64, z as f64 + 0.5]
                    } else {
                        [0.5, -59.0, 0.5]
                    };
                    submit_fixture(
                        &mut world,
                        operator,
                        sequence * 4,
                        Mutation::PlayerCommand(PlayerCommand::Teleport(
                            feet.map(Coordinate::Absolute),
                        )),
                    );
                    world.step_local(Vec::new(), false, true);
                } else {
                    let (from, kind) =
                        if let Some(direction) = operation.strip_prefix("shape_update_from_") {
                            (face(direction), JavaNotification::ShapeUpdate)
                        } else {
                            let source: Vec<i64> = operation
                                .strip_prefix("neighbor_changed_from_")
                                .unwrap()
                                .split('_')
                                .map(|part| part.parse().unwrap())
                                .collect();
                            let source = BlockPos::new(source[0], source[1], source[2]);
                            let from = ["west", "east", "down", "up", "north", "south"]
                                .into_iter()
                                .map(face)
                                .find(|from| from.offset(position) == Some(source))
                                .unwrap();
                            (from, JavaNotification::NeighborChanged)
                        };
                    submit_fixture(
                        &mut world,
                        session,
                        sequence,
                        Mutation::NotifyJavaBlock {
                            target: position,
                            from,
                            kind,
                        },
                    );
                }
            }
            Reference::Sample {
                fixture,
                gt,
                pos: [x, y, z],
                state: Some(state),
            } if fixture.starts_with(prefix) => {
                advance_to(&mut world, gt + gt_offset);
                let position = BlockPos::new(x, y, z);
                let actual = fixture_block_name(&world, position);
                assert_eq!(
                    actual,
                    reference_sample_block(&state),
                    "{fixture} GT {gt} sampled {position:?}"
                );
                samples += 1;
            }
            Reference::BlockTickQueue {
                fixture,
                gt,
                pos: [x, y, z],
                pending,
            } if fixture.starts_with(prefix) => {
                advance_to(&mut world, gt + gt_offset);
                let position = BlockPos::new(x, y, z);
                let actual: Vec<_> = world
                    .pending_java_block_ticks()
                    .filter(|tick| tick.position == position)
                    .collect();
                assert_eq!(
                    actual.len(),
                    pending.len(),
                    "{fixture} GT {gt} queue for {position:?}: {actual:?} != {pending:?}"
                );
                // Vanilla assigns new insertion IDs on load. Compare their relative
                // order within each lifetime, including both reopened ejectors.
                let scope = if prefix == "RESTART-EJECTOR" {
                    prefix.to_owned()
                } else {
                    fixture.clone()
                };
                let orders = queue_orders.entry((reload_epoch, scope)).or_default();
                for (tick, reference) in actual.into_iter().zip(pending) {
                    let block = match tick.kind {
                        JavaBlockTickKind::Lamp => "Block{minecraft:redstone_lamp}",
                        JavaBlockTickKind::Repeater => "Block{minecraft:repeater}",
                        JavaBlockTickKind::Comparator => "Block{minecraft:comparator}",
                        JavaBlockTickKind::Observer => "Block{minecraft:observer}",
                        JavaBlockTickKind::RedstoneTorch => "Block{minecraft:redstone_torch}",
                        JavaBlockTickKind::RedstoneWallTorch => {
                            "Block{minecraft:redstone_wall_torch}"
                        }
                        JavaBlockTickKind::StoneButton => "Block{minecraft:stone_button}",
                        JavaBlockTickKind::OakButton => "Block{minecraft:oak_button}",
                        JavaBlockTickKind::StonePressurePlate => {
                            "Block{minecraft:stone_pressure_plate}"
                        }
                        JavaBlockTickKind::OakPressurePlate => {
                            "Block{minecraft:oak_pressure_plate}"
                        }
                        JavaBlockTickKind::LightPressurePlate => {
                            "Block{minecraft:light_weighted_pressure_plate}"
                        }
                        JavaBlockTickKind::HeavyPressurePlate => {
                            "Block{minecraft:heavy_weighted_pressure_plate}"
                        }
                        JavaBlockTickKind::Dispenser => "Block{minecraft:dispenser}",
                        JavaBlockTickKind::Dropper => "Block{minecraft:dropper}",
                    };
                    assert_eq!(
                        (block, tick.due_gt, tick.priority as i8),
                        (
                            reference.block.as_str(),
                            reference.due_gt + gt_offset,
                            reference.priority
                        ),
                        "{fixture} GT {gt} queue for {position:?}",
                    );
                    if let Some(previous) =
                        orders.insert(reference.sub_tick_order, tick.sub_tick_order)
                    {
                        assert_eq!(
                            previous, tick.sub_tick_order,
                            "{fixture} GT {gt} queue identity"
                        );
                    }
                }
                queue_samples += 1;
            }
            Reference::EjectorInventory {
                fixture,
                gt,
                pos: [x, y, z],
                slots,
            } if fixture.starts_with(prefix) => {
                advance_to(&mut world, gt + gt_offset);
                let pos = BlockPos::new(x, y, z);
                let actual: Vec<_> = world
                    .java_ejector_inventory(pos)
                    .unwrap()
                    .into_iter()
                    .enumerate()
                    .filter_map(|(slot, stack)| {
                        let stack = stack?;
                        let item = world
                            .registry()
                            .block(stack.block)
                            .unwrap()
                            .name
                            .strip_prefix("rustcraft_java:")
                            .unwrap();
                        Some(ReferenceEjectorSlot {
                            slot,
                            count: stack.count,
                            item: item.into(),
                        })
                    })
                    .collect();
                assert_eq!(actual, slots, "{fixture} GT {gt} inventory {pos:?}");
                inventory_samples += 1;
            }
            Reference::EjectedItems {
                fixture,
                gt,
                pos,
                items,
            } if fixture.starts_with(prefix) => {
                advance_to(&mut world, gt + gt_offset);
                let center = pos.map(|axis| axis as f64 + 0.5);
                let actual: Vec<_> = world
                    .snapshot()
                    .dropped_items
                    .into_iter()
                    .filter(|(_, item)| {
                        (0..3).all(|axis| (item.position[axis] - center[axis]).abs() < 3.0)
                    })
                    .collect();
                assert_eq!(
                    actual.len(),
                    items.len(),
                    "{fixture} GT {gt} emitted item count"
                );
                for ((_, entity), expected) in actual.into_iter().zip(items) {
                    let name = world.registry().block(entity.stack.block).unwrap().name;
                    assert_eq!(name, format!("rustcraft_java:{}", expected.item));
                    assert_eq!(entity.stack.count, expected.count);
                    assert_eq!(entity.age, expected.age);
                    assert_eq!(
                        entity.position[0] > center[0] + 0.7,
                        expected.position[0] > center[0] + 0.7,
                        "{fixture} GT {gt} outward emission"
                    );
                }
                entity_samples += 1;
            }
            Reference::PowerQuery {
                fixture,
                gt,
                pos: [x, y, z],
                toward,
                signal,
            } if fixture.starts_with(prefix) => {
                advance_to(&mut world, gt + gt_offset);
                let position = BlockPos::new(x, y, z);
                let actual = world.java_power_output(position, face(&toward)).unwrap();
                assert_eq!(
                    ReferenceSignal {
                        normal: actual.normal.get(),
                        direct: actual.direct.get(),
                        emitted: actual.emitted().unwrap().get(),
                    },
                    signal,
                    "{fixture} GT {gt} {position:?} toward {toward}"
                );
                power_samples += 1;
            }
            _ => {}
        }
    }
    for ((epoch, fixture), orders) in queue_orders {
        assert!(
            orders
                .values()
                .zip(orders.values().skip(1))
                .all(|(first, second)| first < second),
            "{fixture} reload epoch {epoch}: queue insertion order differs: {orders:?}"
        );
    }
    match prefix {
        "TIM-EJECTOR" => assert_eq!((inventory_samples, entity_samples), (15, 10)),
        "RESTART-EJECTOR" => assert_eq!((inventory_samples, entity_samples), (34, 34)),
        _ => {}
    }
    (samples, queue_samples, power_samples)
}

fn load_fixture_ejector(
    world: &mut Server,
    session: SessionId,
    sequence: u64,
    position: BlockPos,
    command: &str,
) {
    let mut slots = [None; 9];
    for stack in command.split("{Slot:").skip(1) {
        let (slot, data) = stack.split_once("b,id:\"minecraft:").unwrap();
        let (name, data) = data.split_once("\",Count:").unwrap();
        let count = data.split_once('b').unwrap().0.parse().unwrap();
        let block = world.registry().block_id(&reference_block(name)).unwrap();
        slots[slot.parse::<usize>().unwrap()] =
            Some(rustcraft_authority::ItemStack { block, count });
    }
    let expected_chunk_revision = world.snapshot().chunks[&position.chunk()].revision();
    submit_fixture(
        world,
        session,
        sequence,
        Mutation::SetJavaEjectorInventory {
            position,
            expected_chunk_revision,
            slots,
        },
    );
}

fn use_fixture_control(
    world: &mut Server,
    operator: SessionId,
    operation: u64,
    position: BlockPos,
    from_below: bool,
) {
    let aimed = submit_fixture(
        world,
        operator,
        operation,
        Mutation::PlayerCommand(PlayerCommand::TeleportFacing {
            position: [
                position.x as f64 + 0.5,
                position.y as f64 + if from_below { -3.0 } else { 1.5 },
                position.z as f64 + 0.5,
            ]
            .map(Coordinate::Absolute),
            rotation: [0.0, if from_below { -90.0 } else { 90.0 }].map(Coordinate::Absolute),
        }),
    );
    let target = aimed.snapshot.player.unwrap().target.unwrap();
    assert_eq!(target.position, position);
    submit_fixture(
        world,
        operator,
        operation + 1,
        Mutation::UseBlock {
            action: BlockAction::Place,
            target,
        },
    );
}

fn apply_fixture_wire_action(
    world: &mut Server,
    session: SessionId,
    operation: u64,
    position: BlockPos,
    placing: bool,
) {
    let dust = world
        .registry()
        .block_id("rustcraft_java:redstone")
        .unwrap();
    if placing {
        submit_fixture(
            world,
            session,
            operation,
            Mutation::PlayerCommand(PlayerCommand::Give {
                item: "minecraft:redstone".into(),
                count: 1,
            }),
        );
    }
    let snapshot = world.snapshot();
    let inventory = snapshot.inventory.unwrap();
    let slot = inventory
        .hotbar()
        .iter()
        .position(|item| {
            if placing {
                item.is_some_and(|item| item.block == dust)
            } else {
                item.is_none()
            }
        })
        .unwrap();
    submit_fixture(
        world,
        session,
        operation + 1,
        Mutation::EditInventory {
            action: InventoryAction::Select { slot },
            expected_revision: inventory.revision(),
        },
    );
    let player = snapshot.player.unwrap();
    let eye_height = player.eye_position()[1] - player.position[1];
    let (feet, pitch) = if placing {
        (
            [
                position.x as f64 + 0.5,
                position.y as f64 + 2.25,
                position.z as f64 + 0.5,
            ],
            90.0,
        )
    } else {
        (
            [
                position.x as f64 + 0.5,
                position.y as f64 + 0.5 - eye_height,
                position.z as f64 - 2.5,
            ],
            0.0,
        )
    };
    let aimed = submit_fixture(
        world,
        session,
        operation + 2,
        Mutation::PlayerCommand(PlayerCommand::TeleportFacing {
            position: feet.map(Coordinate::Absolute),
            rotation: [0.0, pitch].map(Coordinate::Absolute),
        }),
    );
    let target = aimed.snapshot.player.unwrap().target.unwrap();
    let aimed_position = if placing {
        target.placement.unwrap().position
    } else {
        target.position
    };
    assert_eq!(aimed_position, position);
    let report = world.step_local(
        vec![Request {
            session,
            operation: operation + 3,
            mutation: Mutation::UseBlock {
                action: BlockAction::Place,
                target,
            },
        }],
        false,
        false,
    );
    assert!(
        report.acks[0].result.is_ok()
            || (!placing && report.acks[0].result == Err(Rejection::EmptySelectedSlot)),
        "wire {position:?}: {:?}",
        report.acks,
    );
}

fn reference_sample_block(sample: &str) -> String {
    let (block, properties) = sample
        .strip_prefix("Block{minecraft:")
        .unwrap()
        .split_once('}')
        .unwrap();
    reference_block(&format!("{block}{properties}"))
}

fn submit_fixture(
    world: &mut Server,
    session: SessionId,
    operation: u64,
    mutation: Mutation,
) -> rustcraft_authority::TickReport {
    let report = world.step_local(
        vec![Request {
            session,
            operation,
            mutation,
        }],
        false,
        false,
    );
    assert!(report.acks[0].result.is_ok(), "{:?}", report.acks[0]);
    report
}

fn place_fixture_block(
    world: &mut Server,
    host: SessionId,
    operator: SessionId,
    sequence: u64,
    position: BlockPos,
    look: Face,
    item: &str,
) -> String {
    let support = look.offset(position).unwrap();
    let snapshot = world.snapshot();
    let player = snapshot.player.unwrap();
    let eye_height = player.eye_position()[1] - player.position[1];
    let revision = snapshot.chunks[&support.chunk()].revision();
    let state = world
        .registry()
        .block_id("rustcraft_java:smooth_stone")
        .unwrap();
    submit_fixture(
        world,
        host,
        sequence,
        Mutation::SetBlock {
            position: support,
            state,
            expected_revision: revision,
        },
    );
    submit_fixture(
        world,
        operator,
        1,
        Mutation::PlayerCommand(PlayerCommand::Give {
            item: item.into(),
            count: 1,
        }),
    );
    let rotation = match look {
        Face::South => [0.0, 0.0],
        Face::West => [90.0, 0.0],
        Face::North => [180.0, 0.0],
        Face::East => [-90.0, 0.0],
        Face::Up => [0.0, -90.0],
        Face::Down => [0.0, 90.0],
    };
    let feet = [
        position.x as f64 + 0.5 - 3.0 * (support.x - position.x) as f64,
        position.y as f64 + 0.5 - 3.0 * (support.y - position.y) as f64 - eye_height,
        position.z as f64 + 0.5 - 3.0 * (support.z - position.z) as f64,
    ];
    let setup = submit_fixture(
        world,
        operator,
        2,
        Mutation::PlayerCommand(PlayerCommand::TeleportFacing {
            position: feet.map(Coordinate::Absolute),
            rotation: rotation.map(Coordinate::Absolute),
        }),
    );
    let target = setup.snapshot.player.unwrap().target.unwrap();
    assert_eq!(target.placement.unwrap().position, position);
    let placed = submit_fixture(
        world,
        operator,
        3,
        Mutation::UseBlock {
            action: BlockAction::Place,
            target,
        },
    );
    let id = placed.snapshot.chunks[&position.chunk()]
        .block(position)
        .unwrap();
    world.registry().block(id).unwrap().name.into()
}

fn place_east_facing_diode(
    world: &mut Server,
    operator: SessionId,
    operation: u64,
    pos: BlockPos,
    item: &str,
) {
    let player_command = |operation, command| Request {
        session: operator,
        operation,
        mutation: Mutation::PlayerCommand(command),
    };
    let setup = world.step_local(
        vec![
            player_command(
                operation,
                PlayerCommand::Give {
                    item: item.into(),
                    count: 1,
                },
            ),
            player_command(
                operation + 1,
                PlayerCommand::TeleportFacing {
                    position: [pos.x as f64 + 0.5, pos.y as f64 + 1.0, pos.z as f64 + 0.5]
                        .map(Coordinate::Absolute),
                    rotation: [-90.0, 90.0].map(Coordinate::Absolute),
                },
            ),
        ],
        false,
        false,
    );
    assert!(
        setup.acks.iter().all(|ack| ack.result.is_ok()),
        "{:?}",
        setup.acks
    );
    let target = setup.snapshot.player.unwrap().target.unwrap();
    assert_eq!(target.placement.unwrap().position, pos);
    let result = world.step_local(
        vec![Request {
            session: operator,
            operation: operation + 2,
            mutation: Mutation::UseBlock {
                action: BlockAction::Place,
                target,
            },
        }],
        false,
        false,
    );
    assert!(result.acks[0].result.is_ok(), "{:?}", result.acks);
}

fn signal_cell(kind: &str, properties: &str) -> SignalCell {
    let property = |name| {
        properties
            .trim_end_matches(']')
            .split(',')
            .find_map(|property| {
                let (key, value) = property.split_once('=')?;
                (key == name).then_some(value)
            })
    };
    let emitter = match kind {
        "air" | "smooth_stone" | "glass" => Emitter::None,
        "redstone_block" => Emitter::RedstoneBlock,
        "lever" => {
            assert_eq!(property("face"), Some("floor"));
            Emitter::Lever {
                powered: property("powered") == Some("true"),
                support: Face::Down,
            }
        }
        "redstone_wire" => {
            let level =
                property("power").map_or(0, |power| power.parse().expect("wire power value"));
            let connected = |face| matches!(property(face), Some("side" | "up"));
            Emitter::Wire {
                power: Signal::new(level).expect("wire power in 0–15"),
                ports: WirePorts {
                    north: connected("north"),
                    south: connected("south"),
                    west: connected("west"),
                    east: connected("east"),
                },
            }
        }
        _ => panic!("unexpected power fixture block: {kind}"),
    };
    SignalCell {
        conductor: kind == "smooth_stone",
        emitter,
    }
}
