use rustcraft_authority::java_block_ticks::BlockTickPriority;
use rustcraft_authority::java_signal::{Face, PistonPower};
use rustcraft_authority::{
    Capabilities, JavaBlockTickKind, Mutation, PistonAction, PistonKind, PistonTraceStage, Request,
    RulePreset, Server, SessionId,
};
use rustcraft_world::BlockPos;

const BASE: BlockPos = BlockPos::new(15, 16, 6);
const HEAD: BlockPos = BlockPos::new(16, 16, 6);
const LOAD: BlockPos = BlockPos::new(17, 16, 6);
const SOURCE: BlockPos = BlockPos::new(15, 17, 5);
const NOTICE: BlockPos = BlockPos::new(15, 15, 6);

fn edit(world: &mut Server, session: SessionId, operation: u64, position: BlockPos, block: &str) {
    let state = world.registry().block_id(block).unwrap();
    let revision = world.snapshot().chunks[&position.chunk()].revision();
    let report = world.step_local(
        vec![Request {
            session,
            operation,
            mutation: Mutation::SetBlock {
                position,
                state,
                expected_revision: revision,
            },
        }],
        false,
        false,
    );
    assert!(report.acks[0].result.is_ok(), "{:?}", report.acks);
}

fn block_at(world: &Server, position: BlockPos) -> &str {
    let id = world.snapshot().chunks[&position.chunk()]
        .block(position)
        .unwrap();
    world.registry().block(id).unwrap().name
}

fn compare_next_step(world: &mut Server, resumed: &mut Server) {
    let continuous = world.step(Vec::new());
    let restored = resumed.step(Vec::new());
    assert_eq!(continuous.piston_events, restored.piston_events);
    // A restart can remove unused entries from the in-memory block palette.
    assert!(
        world.checkpoint().unwrap() == resumed.checkpoint().unwrap(),
        "restored world differs at GT {}",
        continuous.snapshot.tick,
    );
}

#[test]
fn reopens_quasi_power_pending_events_and_motion_across_chunk_and_section_seams() {
    let preset = RulePreset::JavaFlatV2;
    let mut world = Server::playable_with_rules(7, preset.default_range(), preset).unwrap();
    let session = world.open_local_session(Capabilities::P1_HOST);
    edit(&mut world, session, 1, BASE, "rustcraft_java:piston");
    edit(&mut world, session, 2, HEAD, "rustcraft_java:smooth_stone");
    edit(
        &mut world,
        session,
        3,
        SOURCE,
        "rustcraft_java:redstone_block",
    );
    assert_eq!(
        world.java_piston_power(BASE),
        Some(PistonPower::Quasi(SOURCE))
    );
    assert_eq!(world.pending_piston_events().count(), 0);

    let storage = tempfile::tempdir().unwrap();
    let root = storage.path();
    let generation = rustcraft_persistence::save(root, None, &world).unwrap();
    let (loaded, mut world) = rustcraft_persistence::load_existing(root).unwrap();
    assert_eq!(loaded, generation);
    assert_eq!(world.rules(), preset);
    assert_eq!(
        world.java_piston_power(BASE),
        Some(PistonPower::Quasi(SOURCE))
    );
    for _ in 0..5 {
        assert!(world.step(Vec::new()).piston_events.is_empty());
    }
    assert_eq!(block_at(&world, BASE), "rustcraft_java:piston");

    let session = world.open_local_session(Capabilities::P1_HOST);
    edit(&mut world, session, 1, NOTICE, "rustcraft_java:glass");
    assert_eq!(
        world.pending_piston_events().next().unwrap().action,
        PistonAction::Extend
    );
    let pending_checkpoint = world.checkpoint().unwrap();
    let generation = rustcraft_persistence::save(root, Some(generation), &world).unwrap();
    let (_, mut resumed) = rustcraft_persistence::load_existing(root).unwrap();
    assert_eq!(resumed.checkpoint().unwrap(), pending_checkpoint);
    compare_next_step(&mut world, &mut resumed);
    assert_eq!(
        block_at(&resumed, HEAD),
        "rustcraft_java:moving_piston_east"
    );
    assert_eq!(
        block_at(&resumed, LOAD),
        "rustcraft_java:moving_piston_east"
    );

    let motion_checkpoint = resumed.checkpoint().unwrap();
    let generation = rustcraft_persistence::save(root, Some(generation), &resumed).unwrap();
    let (_, mut restored) = rustcraft_persistence::load_existing(root).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), motion_checkpoint);
    for _ in 0..2 {
        compare_next_step(&mut resumed, &mut restored);
    }
    assert_eq!(block_at(&restored, HEAD), "rustcraft_java:piston_head_east");
    assert_eq!(block_at(&restored, LOAD), "rustcraft_java:smooth_stone");

    let session = restored.open_local_session(Capabilities::P1_HOST);
    edit(&mut restored, session, 1, SOURCE, "rustcraft:empty");
    assert_eq!(restored.pending_piston_events().count(), 0);
    let _ = rustcraft_persistence::save(root, Some(generation), &restored).unwrap();
    let (_, mut restored) = rustcraft_persistence::load_existing(root).unwrap();
    for _ in 0..5 {
        assert!(restored.step(Vec::new()).piston_events.is_empty());
    }
    assert_eq!(block_at(&restored, BASE), "rustcraft_java:piston_extended");
    let session = restored.open_local_session(Capabilities::P1_HOST);
    edit(&mut restored, session, 1, NOTICE, "rustcraft:empty");
    let started = restored.step(Vec::new());
    assert_eq!(started.piston_events[0].stage, PistonTraceStage::Started);
    assert_eq!(
        block_at(&restored, BASE),
        "rustcraft_java:moving_piston_east"
    );
    for _ in 0..2 {
        restored.step(Vec::new());
    }
    assert_eq!(block_at(&restored, BASE), "rustcraft_java:piston");
    assert_eq!(block_at(&restored, LOAD), "rustcraft_java:smooth_stone");
}

#[test]
fn java_reference_v3_reopens_previous_motion_progress_and_discards_queued_events() {
    for elapsed in [1, 2] {
        let preset = RulePreset::JavaFlatV3;
        let mut world = Server::playable_with_rules(7, preset.default_range(), preset).unwrap();
        let session = world.open_local_session(Capabilities::P1_HOST);
        let waiting = BlockPos::new(15, 16, 12);
        let waiting_power = BlockPos::new(15, 17, 11);
        let waiting_notice = BlockPos::new(15, 15, 12);
        edit(&mut world, session, 1, BASE, "rustcraft_java:sticky_piston");
        edit(&mut world, session, 2, HEAD, "rustcraft_java:smooth_stone");
        edit(
            &mut world,
            session,
            3,
            SOURCE,
            "rustcraft_java:redstone_block",
        );
        edit(&mut world, session, 4, NOTICE, "rustcraft_java:glass");
        for _ in 0..elapsed {
            world.step(Vec::new());
        }
        edit(&mut world, session, 5, waiting, "rustcraft_java:piston");
        edit(
            &mut world,
            session,
            6,
            waiting_power,
            "rustcraft_java:redstone_block",
        );
        edit(
            &mut world,
            session,
            7,
            waiting_notice,
            "rustcraft_java:glass",
        );
        assert_eq!(world.pending_piston_events().count(), 1);
        let storage = tempfile::tempdir().unwrap();
        let first = rustcraft_persistence::save(storage.path(), None, &world).unwrap();
        assert_eq!(world.pending_piston_events().count(), 1);
        let (_, restored) = rustcraft_persistence::load_existing(storage.path()).unwrap();
        assert_eq!(restored.rules(), preset);
        assert_eq!(restored.pending_piston_events().count(), 0);
        let resumed = 4 - elapsed;
        let report = restored.inspect_java_bud(BASE);
        assert_eq!(report.motion.len(), 2);
        assert!(
            report
                .motion
                .iter()
                .all(|motion| motion.remaining == resumed
                    && motion.previous_remaining == resumed
                    && motion.awaiting_activation)
        );

        rustcraft_persistence::save(storage.path(), Some(first), &restored).unwrap();
        let (_, mut restored) = rustcraft_persistence::load_existing(storage.path()).unwrap();
        restored.step(Vec::new());
        assert!(
            restored
                .inspect_java_bud(BASE)
                .motion
                .iter()
                .all(|motion| motion.remaining == resumed && !motion.awaiting_activation)
        );
        for _ in 0..resumed - 1 {
            restored.step(Vec::new());
            assert_eq!(
                block_at(&restored, HEAD),
                "rustcraft_java:moving_sticky_piston_east"
            );
        }
        restored.step(Vec::new());
        assert_eq!(
            block_at(&restored, HEAD),
            "rustcraft_java:sticky_piston_head_east"
        );
        assert_eq!(block_at(&restored, LOAD), "rustcraft_java:smooth_stone");
        assert_eq!(block_at(&restored, waiting), "rustcraft_java:piston");
        assert_eq!(restored.pending_piston_events().count(), 0);
        let session = restored.open_local_session(Capabilities::P1_HOST);
        edit(&mut restored, session, 1, waiting_notice, "rustcraft:empty");
        assert_eq!(
            restored.pending_piston_events().next().unwrap().base,
            waiting
        );
    }
}

#[test]
fn rejects_a_world_with_unpaired_moving_piston_markers() {
    let preset = RulePreset::JavaFlatV2;
    let mut world = Server::playable_with_rules(7, preset.default_range(), preset).unwrap();
    let session = world.open_local_session(Capabilities::P1_HOST);
    edit(&mut world, session, 1, BASE, "rustcraft_java:piston");
    edit(
        &mut world,
        session,
        2,
        SOURCE,
        "rustcraft_java:redstone_block",
    );
    edit(&mut world, session, 3, NOTICE, "rustcraft_java:glass");
    world.step(Vec::new());
    let mut checkpoint = world.checkpoint().unwrap();
    checkpoint.java_redstone.as_mut().unwrap().moving.clear();
    let error = Server::restore(checkpoint).err().unwrap();
    assert!(error.contains("missing its Java motion record"));
}

#[test]
fn restores_a_lamp_deadline_from_the_java_block_tick_queue() {
    let preset = RulePreset::JavaFlatV2;
    let mut world = Server::playable_with_rules(7, preset.default_range(), preset).unwrap();
    let session = world.open_local_session(Capabilities::P1_HOST);
    let lamp = BlockPos::new(4, 6, 4);
    let power = BlockPos::new(3, 6, 4);
    edit(&mut world, session, 1, lamp, "rustcraft_java:redstone_lamp");
    edit(
        &mut world,
        session,
        2,
        power,
        "rustcraft_java:redstone_block",
    );
    edit(&mut world, session, 3, power, "rustcraft:empty");
    assert_eq!(world.scheduled_tasks().len(), 0);
    let pending = world.pending_java_block_ticks().next().unwrap();
    assert_eq!(pending.due_gt, 4);
    world.step(Vec::new());
    let storage = tempfile::tempdir().unwrap();
    rustcraft_persistence::save(storage.path(), None, &world).unwrap();
    let (_, mut restored) = rustcraft_persistence::load_existing(storage.path()).unwrap();
    assert_eq!(restored.pending_java_block_ticks().next(), Some(pending));
    for _ in 0..2 {
        compare_next_step(&mut world, &mut restored);
        assert_eq!(block_at(&restored, lamp), "rustcraft_java:redstone_lamp_on");
    }
    compare_next_step(&mut world, &mut restored);
    assert_eq!(block_at(&restored, lamp), "rustcraft_java:redstone_lamp");
    assert_eq!(restored.pending_java_block_ticks().count(), 0);
}

#[test]
fn preserves_powered_wire_dots_and_crosses_in_the_region_palette() {
    let preset = RulePreset::JavaFlatV2;
    let mut world = Server::playable_with_rules(7, preset.default_range(), preset).unwrap();
    let session = world.open_local_session(Capabilities::P1_HOST);
    let dot = BlockPos::new(7, -60, 6);
    let cross = BlockPos::new(10, -60, 6);
    for (operation, (position, name)) in [
        (dot, "rustcraft_java:wire_0_0"),
        (cross, "rustcraft_java:wire_0_40"),
        (
            Face::Up.offset(dot).unwrap(),
            "rustcraft_java:redstone_block",
        ),
        (
            Face::Up.offset(cross).unwrap(),
            "rustcraft_java:redstone_block",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        edit(&mut world, session, operation as u64 + 1, position, name);
    }
    let storage = tempfile::tempdir().unwrap();
    rustcraft_persistence::save(storage.path(), None, &world).unwrap();
    let (_, restored) = rustcraft_persistence::load_existing(storage.path()).unwrap();
    assert_eq!(block_at(&restored, dot), "rustcraft_java:wire_15_0");
    assert_eq!(block_at(&restored, cross), "rustcraft_java:wire_15_40");
    for (position, east) in [(dot, 0), (cross, 15)] {
        assert_eq!(
            restored
                .java_power_output(position, Face::East)
                .unwrap()
                .normal
                .get(),
            east
        );
        assert_eq!(
            restored
                .java_power_output(position, Face::Down)
                .unwrap()
                .normal
                .get(),
            15
        );
    }
}

#[test]
fn restores_a_short_repeater_pulse_before_both_scheduled_edges() {
    let preset = RulePreset::JavaFlatV2;
    let mut world = Server::playable_with_rules(7, preset.default_range(), preset).unwrap();
    let session = world.open_local_session(Capabilities::P1_HOST);
    let support = BlockPos::new(4, 5, 4);
    let repeater = BlockPos::new(4, 6, 4);
    let input = BlockPos::new(3, 6, 4);
    edit(
        &mut world,
        session,
        1,
        support,
        "rustcraft_java:smooth_stone",
    );
    edit(
        &mut world,
        session,
        2,
        repeater,
        "rustcraft_java:repeater_delay3",
    );
    edit(
        &mut world,
        session,
        3,
        input,
        "rustcraft_java:redstone_block",
    );
    world.step(Vec::new());
    edit(&mut world, session, 4, input, "rustcraft:empty");

    for (due, priority, before, after) in [
        (
            6,
            BlockTickPriority::High,
            "rustcraft_java:repeater_delay3",
            "rustcraft_java:repeater_delay3_on",
        ),
        (
            12,
            BlockTickPriority::VeryHigh,
            "rustcraft_java:repeater_delay3_on",
            "rustcraft_java:repeater_delay3",
        ),
    ] {
        let pending = world.pending_java_block_ticks().next().unwrap();
        assert_eq!(
            (pending.due_gt, pending.priority, pending.kind),
            (due, priority, JavaBlockTickKind::Repeater)
        );
        assert_eq!(world.scheduled_tasks().len(), 0);
        let storage = tempfile::tempdir().unwrap();
        rustcraft_persistence::save(storage.path(), None, &world).unwrap();
        let (_, mut restored) = rustcraft_persistence::load_existing(storage.path()).unwrap();
        assert_eq!(restored.pending_java_block_ticks().next(), Some(pending));
        while restored.snapshot().tick + 1 < due {
            compare_next_step(&mut world, &mut restored);
            assert_eq!(block_at(&restored, repeater), before);
        }
        compare_next_step(&mut world, &mut restored);
        assert_eq!(block_at(&restored, repeater), after);
        world = restored;
    }
    assert_eq!(world.pending_java_block_ticks().count(), 0);
}

#[test]
fn restores_observer_edges_and_their_output_across_a_section_boundary() {
    let preset = RulePreset::JavaFlatV2;
    let mut world = Server::playable_with_rules(7, preset.default_range(), preset).unwrap();
    let session = world.open_local_session(Capabilities::P1_HOST);
    let observer = BlockPos::new(4, 16, 4);
    let watched = BlockPos::new(4, 17, 4);
    let lamp = BlockPos::new(4, 15, 4);
    edit(
        &mut world,
        session,
        1,
        observer,
        "rustcraft_java:observer_up",
    );
    edit(&mut world, session, 2, lamp, "rustcraft_java:redstone_lamp");
    for _ in 0..8 {
        world.step(Vec::new());
    }
    assert_eq!(world.pending_java_block_ticks().count(), 0);
    edit(&mut world, session, 3, watched, "rustcraft_java:glass");
    world.step(Vec::new());

    for (due_gt, state) in [
        (10, "rustcraft_java:observer_up_on"),
        (12, "rustcraft_java:observer_up"),
    ] {
        let pending = world.pending_java_block_ticks().next().unwrap();
        assert_eq!(pending.kind, JavaBlockTickKind::Observer);
        assert_eq!(pending.priority, BlockTickPriority::Normal);
        assert_eq!(pending.due_gt, due_gt);
        let storage = tempfile::tempdir().unwrap();
        rustcraft_persistence::save(storage.path(), None, &world).unwrap();
        let (_, mut restored) = rustcraft_persistence::load_existing(storage.path()).unwrap();
        assert_eq!(restored.pending_java_block_ticks().next(), Some(pending));
        while restored.snapshot().tick < due_gt {
            compare_next_step(&mut world, &mut restored);
        }
        assert_eq!(block_at(&restored, observer), state);
        assert_eq!(block_at(&restored, lamp), "rustcraft_java:redstone_lamp_on");
        world = restored;
    }
    for _ in 0..4 {
        world.step(Vec::new());
    }
    assert_eq!(block_at(&world, lamp), "rustcraft_java:redstone_lamp");
    assert_eq!(world.pending_java_block_ticks().count(), 0);
}

#[test]
fn preserves_comparator_strength_and_its_next_tick_in_a_resident_inactive_chunk() {
    use rustcraft_authority::java_signal::Face;

    let preset = RulePreset::JavaFlatV2;
    let mut world = Server::playable_with_rules(7, preset.default_range(), preset).unwrap();
    let session = world.open_local_session(Capabilities::P1_HOST);
    let comparator = BlockPos::new(16, 21, 12);
    let cake = BlockPos::new(15, 21, 12);
    edit(
        &mut world,
        session,
        1,
        BlockPos::new(16, 20, 12),
        "rustcraft_java:smooth_stone",
    );
    edit(
        &mut world,
        session,
        2,
        BlockPos::new(15, 20, 12),
        "rustcraft_java:smooth_stone",
    );
    edit(
        &mut world,
        session,
        3,
        comparator,
        "rustcraft_java:comparator",
    );
    edit(&mut world, session, 4, cake, "rustcraft_java:cake_bites_1");
    world.step(Vec::new());
    world.step(Vec::new());
    assert_eq!(
        world
            .java_power_output(comparator, Face::East)
            .unwrap()
            .normal
            .get(),
        12
    );
    edit(&mut world, session, 5, cake, "rustcraft_java:cake_bites_4");
    let pending = world.pending_java_block_ticks().next().unwrap();
    assert_eq!(pending.kind, JavaBlockTickKind::Comparator);

    let set_active = |world: &mut Server, session, operation, active| {
        let report = world.step_local(
            vec![Request {
                session,
                operation,
                mutation: Mutation::SetChunkActive {
                    position: comparator.chunk(),
                    active,
                },
            }],
            false,
            false,
        );
        assert!(report.acks[0].result.is_ok(), "{:?}", report.acks[0]);
    };
    set_active(&mut world, session, 6, false);
    let storage = tempfile::tempdir().unwrap();
    rustcraft_persistence::save(storage.path(), None, &world).unwrap();
    let (_, mut restored) = rustcraft_persistence::load_existing(storage.path()).unwrap();
    assert_eq!(restored.pending_java_block_ticks().next(), Some(pending));
    assert!(restored.java_power_output(comparator, Face::East).is_none());
    let restored_session = restored.open_local_session(Capabilities::P1_HOST);
    set_active(&mut restored, restored_session, 1, true);
    assert_eq!(
        restored
            .java_power_output(comparator, Face::East)
            .unwrap()
            .normal
            .get(),
        12
    );
    restored.step(Vec::new());
    assert_eq!(
        restored
            .java_power_output(comparator, Face::East)
            .unwrap()
            .normal
            .get(),
        12
    );
    restored.step(Vec::new());
    assert_eq!(
        restored
            .java_power_output(comparator, Face::East)
            .unwrap()
            .normal
            .get(),
        6
    );
    assert_eq!(restored.pending_java_block_ticks().count(), 0);
}

#[test]
fn restores_a_pressed_button_and_plate_with_their_original_deadlines() {
    use rustcraft_authority::java_signal::Face;
    use rustcraft_authority::{BlockAction, Coordinate, PlayerCommand};

    let preset = RulePreset::JavaFlatV2;
    let mut world = Server::playable_with_rules(7, preset.default_range(), preset).unwrap();
    let host = world.open_local_session(Capabilities::P1_HOST);
    let operator = world.open_local_operator().unwrap();
    let button = BlockPos::new(4, 16, 4);
    let plate = BlockPos::new(8, 16, 4);
    edit(
        &mut world,
        host,
        1,
        BlockPos::new(4, 15, 4),
        "rustcraft_java:smooth_stone",
    );
    edit(
        &mut world,
        host,
        2,
        BlockPos::new(8, 15, 4),
        "rustcraft_java:smooth_stone",
    );
    edit(
        &mut world,
        host,
        3,
        button,
        "rustcraft_java:stone_button_floor_south",
    );
    edit(
        &mut world,
        host,
        4,
        plate,
        "rustcraft_java:light_weighted_pressure_plate",
    );
    let input = |world: &mut Server, session, operation, mutation| {
        let report = world.step_local(
            vec![Request {
                session,
                operation,
                mutation,
            }],
            false,
            false,
        );
        assert!(report.acks[0].result.is_ok(), "{:?}", report.acks);
        report.snapshot
    };
    let aimed = input(
        &mut world,
        operator,
        1,
        Mutation::PlayerCommand(PlayerCommand::TeleportFacing {
            position: [4.5, 17.5, 4.5].map(Coordinate::Absolute),
            rotation: [0.0, 90.0].map(Coordinate::Absolute),
        }),
    );
    let target = aimed.player.unwrap().target.unwrap();
    assert_eq!(target.position, button);
    input(
        &mut world,
        operator,
        2,
        Mutation::UseBlock {
            action: BlockAction::Place,
            target,
        },
    );
    input(
        &mut world,
        operator,
        3,
        Mutation::PlayerCommand(PlayerCommand::Teleport(
            [8.5, 16.0, 4.5].map(Coordinate::Absolute),
        )),
    );
    world.step_local(Vec::new(), false, true);
    let deadlines: Vec<_> = world
        .pending_java_block_ticks()
        .map(|tick| (tick.kind, tick.due_gt))
        .collect();
    assert_eq!(
        deadlines,
        vec![
            (JavaBlockTickKind::LightPressurePlate, 10),
            (JavaBlockTickKind::StoneButton, 20)
        ]
    );

    let storage = tempfile::tempdir().unwrap();
    rustcraft_persistence::save(storage.path(), None, &world).unwrap();
    let (_, mut restored) = rustcraft_persistence::load_existing(storage.path()).unwrap();
    assert!(restored.checkpoint().unwrap() == world.checkpoint().unwrap());
    let operator = restored.open_local_operator().unwrap();
    input(
        &mut restored,
        operator,
        1,
        Mutation::PlayerCommand(PlayerCommand::Teleport(
            [0.5, -59.0, 0.5].map(Coordinate::Absolute),
        )),
    );
    for (position, deadline, signal) in [(plate, 10, 1), (button, 20, 15)] {
        while restored.snapshot().tick + 1 < deadline {
            restored.step(Vec::new());
        }
        assert_eq!(
            restored
                .java_power_output(position, Face::Down)
                .unwrap()
                .direct
                .get(),
            signal
        );
        restored.step(Vec::new());
        assert_eq!(
            restored
                .java_power_output(position, Face::Down)
                .unwrap()
                .direct
                .get(),
            0
        );
    }
    assert_eq!(restored.pending_java_block_ticks().count(), 0);
}

#[test]
fn restores_a_dropper_pulse_and_its_airborne_item_across_the_chunk_seam() {
    use rustcraft_authority::ItemStack;

    let preset = RulePreset::JavaFlatV2;
    let mut world = Server::playable_with_rules(7, preset.default_range(), preset).unwrap();
    let host = world.open_local_session(Capabilities::P1_HOST);
    let source = BlockPos::new(15, 22, 6);
    let input = BlockPos::new(14, 22, 6);
    edit(&mut world, host, 1, source, "rustcraft_java:dropper_east");
    let block = world
        .registry()
        .block_id("rustcraft_java:smooth_stone")
        .unwrap();
    let mut slots = [None; 9];
    slots[4] = Some(ItemStack { block, count: 2 });
    let expected_chunk_revision = world.snapshot().chunks[&source.chunk()].revision();
    let report = world.step_local(
        vec![Request {
            session: host,
            operation: 2,
            mutation: Mutation::SetJavaEjectorInventory {
                position: source,
                expected_chunk_revision,
                slots,
            },
        }],
        false,
        false,
    );
    assert!(report.acks[0].result.is_ok());
    edit(&mut world, host, 3, input, "rustcraft_java:redstone_block");
    world.step(Vec::new());
    edit(&mut world, host, 4, input, "rustcraft:empty");
    let deadline = world.pending_java_block_ticks().next().unwrap();
    assert_eq!(deadline.due_gt, 4);
    let storage = tempfile::tempdir().unwrap();
    rustcraft_persistence::save(storage.path(), None, &world).unwrap();
    let (_, mut reopened) = rustcraft_persistence::load_existing(storage.path()).unwrap();
    assert_eq!(reopened.java_ejector_inventory(source), Some(slots));
    assert_eq!(reopened.pending_java_block_ticks().next(), Some(deadline));
    for _ in 0..3 {
        compare_next_step(&mut world, &mut reopened);
    }
    let snapshot = reopened.snapshot();
    assert_eq!(snapshot.dropped_items.len(), 1);
    let (_, item) = snapshot.dropped_items[0];
    assert_eq!(item.stack, ItemStack { block, count: 1 });
    assert_eq!(item.age, 1);
    assert!(item.position[0] > 16.0);
    assert_eq!(
        reopened.java_ejector_inventory(source).unwrap()[4]
            .unwrap()
            .count,
        1
    );

    let flight = tempfile::tempdir().unwrap();
    rustcraft_persistence::save(flight.path(), None, &reopened).unwrap();
    let (_, mut flight) = rustcraft_persistence::load_existing(flight.path()).unwrap();
    assert_eq!(flight.snapshot().dropped_items, snapshot.dropped_items);
    compare_next_step(&mut world, &mut flight);
}

#[test]
fn preserves_the_scheduled_relight_of_a_burned_wall_torch() {
    let preset = RulePreset::JavaFlatV2;
    let mut world = Server::playable_with_rules(7, preset.default_range(), preset).unwrap();
    let session = world.open_local_session(Capabilities::P1_HOST);
    let torch = BlockPos::new(16, 18, 12);
    let support = BlockPos::new(15, 18, 12);
    edit(
        &mut world,
        session,
        1,
        support,
        "rustcraft_java:smooth_stone",
    );
    edit(
        &mut world,
        session,
        2,
        torch,
        "rustcraft_java:redstone_wall_torch_east",
    );
    for pulse in 0..8 {
        edit(
            &mut world,
            session,
            2 * pulse + 3,
            support,
            "rustcraft_java:redstone_block",
        );
        world.step(Vec::new());
        world.step(Vec::new());
        edit(
            &mut world,
            session,
            2 * pulse + 4,
            support,
            "rustcraft_java:smooth_stone",
        );
        if pulse < 7 {
            world.step(Vec::new());
            world.step(Vec::new());
        }
    }
    let pending = world.pending_java_block_ticks().next().unwrap();
    assert_eq!(pending.kind, JavaBlockTickKind::RedstoneWallTorch);
    assert_eq!(pending.due_gt - world.snapshot().tick, 160);
    let storage = tempfile::tempdir().unwrap();
    rustcraft_persistence::save(storage.path(), None, &world).unwrap();
    let (_, mut restored) = rustcraft_persistence::load_existing(storage.path()).unwrap();
    assert_eq!(restored.pending_java_block_ticks().next(), Some(pending));
    while restored.snapshot().tick + 1 < pending.due_gt {
        restored.step(Vec::new());
    }
    assert_eq!(
        block_at(&restored, torch),
        "rustcraft_java:redstone_wall_torch_east_off"
    );
    restored.step(Vec::new());
    assert_eq!(
        block_at(&restored, torch),
        "rustcraft_java:redstone_wall_torch_east"
    );
    assert_eq!(restored.pending_java_block_ticks().count(), 0);
}

#[test]
fn restores_sticky_piston_drop_and_pull_across_a_chunk_seam() {
    let preset = RulePreset::JavaFlatV2;
    for (pulse_gt, action) in [(2, PistonAction::Drop), (3, PistonAction::Retract)] {
        let mut world = Server::playable_with_rules(7, preset.default_range(), preset).unwrap();
        let session = world.open_local_session(Capabilities::P1_HOST);
        let input = BlockPos::new(BASE.x - 1, BASE.y, BASE.z);
        edit(&mut world, session, 1, BASE, "rustcraft_java:sticky_piston");
        edit(&mut world, session, 2, HEAD, "rustcraft_java:smooth_stone");
        edit(
            &mut world,
            session,
            3,
            input,
            "rustcraft_java:redstone_block",
        );
        for _ in 0..pulse_gt {
            world.step(Vec::new());
        }
        edit(&mut world, session, 4, input, "rustcraft:empty");
        let pending = world.pending_piston_events().next().unwrap();
        assert_eq!((pending.kind, pending.action), (PistonKind::Sticky, action));

        let storage = tempfile::tempdir().unwrap();
        rustcraft_persistence::save(storage.path(), None, &world).unwrap();
        let (_, mut restored) = rustcraft_persistence::load_existing(storage.path()).unwrap();
        assert_eq!(restored.pending_piston_events().next(), Some(pending));
        compare_next_step(&mut world, &mut restored);
        assert_eq!(
            block_at(&restored, BASE),
            "rustcraft_java:moving_sticky_piston_east"
        );
        let motion_storage = tempfile::tempdir().unwrap();
        rustcraft_persistence::save(motion_storage.path(), None, &restored).unwrap();
        let (_, mut restored) =
            rustcraft_persistence::load_existing(motion_storage.path()).unwrap();
        for _ in 0..2 {
            compare_next_step(&mut world, &mut restored);
        }
        let (head, load) = if action == PistonAction::Drop {
            ("rustcraft:empty", "rustcraft_java:smooth_stone")
        } else {
            ("rustcraft_java:smooth_stone", "rustcraft:empty")
        };
        assert_eq!(block_at(&restored, BASE), "rustcraft_java:sticky_piston");
        assert_eq!(block_at(&restored, HEAD), head);
        assert_eq!(block_at(&restored, LOAD), load);
    }
}

#[test]
fn opens_java_motion_from_each_legacy_redstone_record() {
    let preset = RulePreset::JavaFlatV2;
    let mut world = Server::playable_with_rules(7, preset.default_range(), preset).unwrap();
    let session = world.open_local_session(Capabilities::P1_HOST);
    edit(&mut world, session, 1, BASE, "rustcraft_java:piston");
    edit(&mut world, session, 2, HEAD, "rustcraft_java:smooth_stone");
    edit(
        &mut world,
        session,
        3,
        BlockPos::new(BASE.x - 1, BASE.y, BASE.z),
        "rustcraft_java:redstone_block",
    );
    world.step(Vec::new());
    let storage = tempfile::tempdir().unwrap();
    let generation = rustcraft_persistence::save(storage.path(), None, &world).unwrap();
    let path = storage
        .path()
        .join(format!("checkpoints/{generation}/manifest.json"));
    let checkpoint = world.checkpoint().unwrap();
    let bytes = std::fs::read(&path).unwrap();
    for version in [1, 2, 3] {
        let mut manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let redstone = manifest["java_redstone"].as_object_mut().unwrap();
        redstone.insert("schema".into(), version.into());
        if version < 3 {
            redstone.remove("comparator_outputs").unwrap();
        }
        redstone.remove("next_entity").unwrap();
        redstone.remove("ejector_inventories").unwrap();
        redstone.remove("dropped_items").unwrap();
        let moving = redstone.get_mut("moving").unwrap().as_array_mut().unwrap();
        assert_eq!(moving.len(), 2);
        if version == 1 {
            for motion in moving {
                motion["event"]
                    .as_object_mut()
                    .unwrap()
                    .remove("kind")
                    .unwrap();
            }
        }
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let (_, mut restored) = rustcraft_persistence::load_existing(storage.path()).unwrap();
        assert!(restored.checkpoint().unwrap() == checkpoint);
        let mut continuous = Server::restore(checkpoint.clone()).unwrap();
        for _ in 0..2 {
            compare_next_step(&mut continuous, &mut restored);
        }
        assert_eq!(block_at(&restored, HEAD), "rustcraft_java:piston_head_east");
    }
}
