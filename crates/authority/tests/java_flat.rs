use rustcraft_authority::{
    BlockAction, Capabilities, Coordinate, Effect, GameMode, InventoryAction, InventoryButton,
    JavaBlockTickKind, Mutation, PlayerCommand, PlayerInput, Rejection, Request, RulePreset,
    Server, SessionId,
};
use rustcraft_world::{BlockPos, ChunkPos};

fn with_redstone_world(
    test: impl FnOnce(&mut Server, &mut dyn FnMut(&mut Server, BlockPos, &str)),
) {
    let preset = RulePreset::JavaFlatV1;
    let mut world = Server::playable_with_rules(42, preset.default_range(), preset).unwrap();
    let session = world.open_local_session(Capabilities::P1_HOST);
    let mut operation = 0;
    test(&mut world, &mut |world, position, name| {
        operation += 1;
        let expected_revision = world.snapshot().chunks[&position.chunk()].revision();
        let state = world.registry().block_id(name).unwrap();
        let report = world.step(vec![Request {
            session,
            operation,
            mutation: Mutation::SetBlock {
                position,
                expected_revision,
                state,
            },
        }]);
        assert_eq!(
            report.acks[0].result,
            Ok(Effect::ChunkChanged(position.chunk()))
        );
    });
}

fn state_at(world: &Server, position: BlockPos) -> &str {
    let state = world.snapshot().chunks[&position.chunk()]
        .block(position)
        .unwrap();
    world.registry().block(state).unwrap().name
}

fn submit_set_block(
    server: &mut Server,
    session: SessionId,
    operation: &mut u64,
    position: BlockPos,
    name: &str,
) {
    *operation += 1;
    let expected_revision = server.snapshot().chunks[&position.chunk()].revision();
    let report = server.step(vec![Request {
        session,
        operation: *operation,
        mutation: Mutation::SetBlock {
            position,
            expected_revision,
            state: server.registry().block_id(name).unwrap(),
        },
    }]);
    assert_eq!(
        report.acks[0].result,
        Ok(Effect::ChunkChanged(position.chunk()))
    );
}

fn submit_chunk_active(
    server: &mut Server,
    session: SessionId,
    operation: &mut u64,
    position: ChunkPos,
    active: bool,
) {
    *operation += 1;
    let report = server.step(vec![Request {
        session,
        operation: *operation,
        mutation: Mutation::SetChunkActive { position, active },
    }]);
    assert!(report.acks[0].result.is_ok());
}

fn submit_set_block_and_deactivate(
    server: &mut Server,
    session: SessionId,
    operation: &mut u64,
    position: BlockPos,
    name: &str,
    chunk: ChunkPos,
) {
    *operation += 1;
    let set = Request {
        session,
        operation: *operation,
        mutation: Mutation::SetBlock {
            position,
            expected_revision: server.snapshot().chunks[&position.chunk()].revision(),
            state: server.registry().block_id(name).unwrap(),
        },
    };
    *operation += 1;
    let deactivate = Request {
        session,
        operation: *operation,
        mutation: Mutation::SetChunkActive {
            position: chunk,
            active: false,
        },
    };
    let report = server.step(vec![set, deactivate]);
    assert!(report.acks.iter().all(|ack| ack.result.is_ok()));
}

#[test]
fn dust_propagates_up_and_down_across_a_chunk_seam_and_stops_when_covered() {
    with_redstone_world(|world, put| {
        let low = BlockPos::new(15, -60, 5);
        let high = BlockPos::new(16, -59, 5);
        let lower_lever = BlockPos::new(14, -60, 5);
        let upper_lever = BlockPos::new(17, -59, 5);
        put(
            world,
            BlockPos::new(16, -60, 5),
            "rustcraft_java:smooth_stone",
        );
        put(
            world,
            BlockPos::new(17, -60, 5),
            "rustcraft_java:smooth_stone",
        );
        put(world, low, "rustcraft_java:redstone");
        put(world, high, "rustcraft_java:redstone");
        put(world, lower_lever, "rustcraft_java:lever_on");
        assert_eq!(state_at(world, low), "rustcraft_java:redstone_power_15");
        assert_eq!(state_at(world, high), "rustcraft_java:redstone_power_14");
        let cover = BlockPos::new(15, -59, 5);
        put(world, cover, "rustcraft_java:smooth_stone");
        assert_eq!(state_at(world, high), "rustcraft_java:redstone");
        put(world, cover, "rustcraft:empty");
        assert_eq!(state_at(world, high), "rustcraft_java:redstone_power_14");
        put(world, lower_lever, "rustcraft_java:lever");
        put(world, upper_lever, "rustcraft_java:lever_on");
        assert_eq!(state_at(world, high), "rustcraft_java:redstone_power_15");
        assert_eq!(state_at(world, low), "rustcraft_java:redstone_power_14");
    });
}

#[test]
fn preserves_lamp_and_piston_events_across_an_inactive_chunk() {
    let preset = RulePreset::JavaFlatV3;
    let mut server = Server::playable_with_rules(42, preset.default_range(), preset).unwrap();
    let session = server.open_local_session(Capabilities::P1_HOST);
    let mut operation = 0;
    let chunk = ChunkPos { x: 1, z: 0 };
    let lamp = BlockPos::new(20, -60, 5);
    let lamp_power = BlockPos::new(19, -60, 5);
    submit_set_block(
        &mut server,
        session,
        &mut operation,
        lamp,
        "rustcraft_java:redstone_lamp",
    );
    submit_set_block(
        &mut server,
        session,
        &mut operation,
        lamp_power,
        "rustcraft_java:redstone_block",
    );
    assert_eq!(state_at(&server, lamp), "rustcraft_java:redstone_lamp_on");
    submit_set_block(
        &mut server,
        session,
        &mut operation,
        lamp_power,
        "rustcraft:empty",
    );
    assert!(
        server
            .pending_java_block_ticks()
            .any(|tick| tick.position == lamp && tick.kind == JavaBlockTickKind::Lamp)
    );
    submit_chunk_active(&mut server, session, &mut operation, chunk, false);
    for _ in 0..5 {
        server.step(Vec::new());
    }
    assert!(!server.snapshot().chunks.contains_key(&chunk));
    assert!(
        server
            .pending_java_block_ticks()
            .any(|tick| tick.position == lamp && tick.kind == JavaBlockTickKind::Lamp)
    );
    submit_chunk_active(&mut server, session, &mut operation, chunk, true);
    server.step(Vec::new());
    assert_eq!(state_at(&server, lamp), "rustcraft_java:redstone_lamp");
    assert!(
        !server
            .pending_java_block_ticks()
            .any(|tick| tick.position == lamp && tick.kind == JavaBlockTickKind::Lamp)
    );

    let piston = BlockPos::new(20, -60, 7);
    let head = BlockPos::new(21, -60, 7);
    let load = BlockPos::new(22, -60, 7);
    let piston_power = BlockPos::new(19, -60, 7);
    submit_set_block(
        &mut server,
        session,
        &mut operation,
        piston,
        "rustcraft_java:piston",
    );
    submit_set_block(
        &mut server,
        session,
        &mut operation,
        load,
        "rustcraft_java:smooth_stone",
    );
    submit_set_block_and_deactivate(
        &mut server,
        session,
        &mut operation,
        piston_power,
        "rustcraft_java:redstone_block",
        chunk,
    );
    assert!(
        server
            .pending_piston_events()
            .any(|event| event.base == piston)
    );
    for _ in 0..2 {
        server.step(Vec::new());
    }
    assert!(
        server
            .pending_piston_events()
            .any(|event| event.base == piston)
    );
    submit_chunk_active(&mut server, session, &mut operation, chunk, true);
    for _ in 0..2 {
        server.step(Vec::new());
    }
    assert_eq!(state_at(&server, piston), "rustcraft_java:piston_extended");
    assert_eq!(state_at(&server, head), "rustcraft_java:piston_head_east");
    assert_eq!(state_at(&server, load), "rustcraft_java:smooth_stone");
    assert!(
        !server
            .pending_piston_events()
            .any(|event| event.base == piston)
    );
}

#[test]
fn preserves_a_piston_event_across_a_section_boundary() {
    let preset = RulePreset::JavaFlatV3;
    let mut server = Server::playable_with_rules(42, preset.default_range(), preset).unwrap();
    let session = server.open_local_session(Capabilities::P1_HOST);
    let mut operation = 0;
    let chunk = ChunkPos { x: 1, z: 0 };
    let piston = BlockPos::new(20, -49, 9);
    let head = BlockPos::new(20, -48, 9);
    let load = BlockPos::new(20, -47, 9);
    let power = BlockPos::new(19, -49, 9);
    submit_set_block(
        &mut server,
        session,
        &mut operation,
        piston,
        "rustcraft_java:piston_up",
    );
    submit_set_block(
        &mut server,
        session,
        &mut operation,
        load,
        "rustcraft_java:smooth_stone",
    );
    submit_set_block_and_deactivate(
        &mut server,
        session,
        &mut operation,
        power,
        "rustcraft_java:redstone_block",
        chunk,
    );
    assert!(
        server
            .pending_piston_events()
            .any(|event| event.base == piston)
    );
    server.step(Vec::new());
    submit_chunk_active(&mut server, session, &mut operation, chunk, true);
    for _ in 0..2 {
        server.step(Vec::new());
    }
    assert_eq!(
        state_at(&server, piston),
        "rustcraft_java:piston_up_extended"
    );
    assert_eq!(state_at(&server, head), "rustcraft_java:piston_head_up");
    assert_eq!(state_at(&server, load), "rustcraft_java:smooth_stone");
}

#[test]
fn piston_clears_powered_and_unpowered_dust_and_breaks_the_linked_pair() {
    for powered in [false, true] {
        with_redstone_world(|world, put| {
            let piston = BlockPos::new(15, -60, 3);
            let head = BlockPos::new(16, -60, 3);
            put(world, head, "rustcraft_java:redstone");
            if powered {
                put(world, BlockPos::new(16, -60, 4), "rustcraft_java:lever_on");
                assert_eq!(state_at(world, head), "rustcraft_java:redstone_power_15");
            }
            put(world, piston, "rustcraft_java:piston");
            put(world, BlockPos::new(14, -60, 3), "rustcraft_java:lever_on");
            assert_eq!(state_at(world, piston), "rustcraft_java:piston_extended");
            assert_eq!(state_at(world, head), "rustcraft_java:piston_head_east");
            assert_eq!(
                state_at(world, BlockPos::new(17, -60, 3)),
                "rustcraft:empty"
            );
            let broken = if powered { head } else { piston };
            put(world, broken, "rustcraft:empty");
            assert_eq!(state_at(world, piston), "rustcraft:empty");
            assert_eq!(state_at(world, head), "rustcraft:empty");
        });
    }
}

#[test]
fn samples_classic_flat_layers_at_negative_and_positive_chunk_boundaries() {
    let preset = RulePreset::JavaFlatV1;
    let layers = [
        (-64, "rustcraft_java:bedrock"),
        (-63, "rustcraft_java:dirt"),
        (-62, "rustcraft_java:dirt"),
        (-61, "rustcraft_java:grass_block"),
        (-60, "rustcraft:empty"),
    ];
    for seed in [0, 42] {
        let server = Server::playable_with_rules(seed, preset.default_range(), preset).unwrap();
        let saved = server.checkpoint().unwrap();
        let restored = Server::restore(saved.clone()).unwrap();
        assert_eq!(restored.checkpoint().unwrap(), saved);
        assert_eq!(restored.rules(), preset);
        let snapshot = restored.snapshot();
        for x in [-16, -1, 0, 15, 16] {
            for z in [-16, -1, 0, 15, 16] {
                for (y, expected) in layers {
                    let pos = BlockPos::new(x, y, z);
                    let id = snapshot.chunks[&pos.chunk()].block(pos).unwrap();
                    assert_eq!(
                        restored.registry().block(id).unwrap().name,
                        expected,
                        "{pos:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn runs_cross_chunk_signals_and_replays_a_pending_piston_line_after_restore() {
    let preset = RulePreset::JavaFlatV1;
    let mut server = Server::playable_with_rules(42, preset.default_range(), preset).unwrap();
    let session = server.open_local_session(Capabilities::P1_HOST);
    let state = |server: &Server, x| {
        let pos = BlockPos::new(x, -60, 3);
        let id = server.snapshot().chunks[&pos.chunk()].block(pos).unwrap();
        server.registry().block(id).unwrap().name
    };
    let mut operation = 0;
    let mut put = |server: &mut Server, x, name| {
        let position = BlockPos::new(x, -60, 3);
        let expected_revision = server.snapshot().chunks[&position.chunk()].revision();
        operation += 1;
        let report = server.step(vec![Request {
            session,
            operation,
            mutation: Mutation::SetBlock {
                position,
                expected_revision,
                state: server.registry().block_id(name).unwrap(),
            },
        }]);
        assert!(matches!(report.acks[0].result, Ok(Effect::ChunkChanged(_))));
        report
    };
    for (x, name) in [
        (13, "rustcraft_java:lever"),
        (14, "rustcraft_java:redstone"),
        (15, "rustcraft_java:redstone"),
        (16, "rustcraft_java:repeater"),
        (17, "rustcraft_java:piston"),
        (18, "rustcraft_java:smooth_stone"),
        (12, "rustcraft_java:redstone_lamp"),
    ] {
        put(&mut server, x, name);
    }
    let trigger = put(&mut server, 13, "rustcraft_java:lever_on");
    assert_eq!(state(&server, 14), "rustcraft_java:redstone_power_15");
    assert_eq!(state(&server, 15), "rustcraft_java:redstone_power_14");
    assert_eq!(state(&server, 17), "rustcraft_java:piston");
    assert_eq!(state(&server, 12), "rustcraft_java:redstone_lamp_on");
    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    assert!(
        trigger
            .redstone_updates
            .iter()
            .any(|signal| signal.power == 14)
    );
    assert_eq!(server.scheduled_tasks().len(), 1);
    let mut restored = Server::restore(server.checkpoint().unwrap()).unwrap();
    for _ in 0..2 {
        let a = server.step(Vec::new());
        let b = restored.step(Vec::new());
        assert_eq!(a.task_events, b.task_events);
        assert_eq!(a.due_tasks, b.due_tasks);
        #[cfg(any(debug_assertions, feature = "dev-tools"))]
        {
            assert_eq!(a.block_updates, b.block_updates);
            assert_eq!(a.redstone_updates, b.redstone_updates);
        }
        assert_eq!(server.checkpoint().unwrap(), restored.checkpoint().unwrap());
    }
    assert_eq!(state(&server, 16), "rustcraft_java:repeater_on");
    assert_eq!(state(&server, 17), "rustcraft_java:piston_extended");
    assert_eq!(state(&server, 18), "rustcraft_java:piston_head_east");
    assert_eq!(state(&server, 19), "rustcraft_java:smooth_stone");
    assert_eq!(server.snapshot().tick, trigger.snapshot.tick + 2);
    put(&mut server, 13, "rustcraft_java:lever");
    for _ in 0..2 {
        server.step(Vec::new());
    }
    assert_eq!(state(&server, 17), "rustcraft_java:piston");
    assert_eq!(state(&server, 18), "rustcraft:empty");
    assert_eq!(state(&server, 19), "rustcraft_java:smooth_stone");
    assert_eq!(state(&server, 12), "rustcraft_java:redstone_lamp_on");
    for _ in 0..2 {
        server.step(Vec::new());
    }
    assert_eq!(state(&server, 12), "rustcraft_java:redstone_lamp");
}

#[test]
fn picks_an_existing_hotbar_stack_and_places_and_mines_with_cancellable_progress() {
    let preset = RulePreset::JavaFlatV1;
    let mut world = Server::playable_with_rules(42, preset.default_range(), preset).unwrap();
    let session = world.open_local_operator().unwrap();
    let mut operation = 0;
    let mut send = |world: &mut Server, mutation| {
        operation += 1;
        let result = world.step(vec![Request {
            session,
            operation,
            mutation,
        }]);
        assert!(result.acks[0].result.is_ok(), "{:?}", result.acks[0]);
        result
    };
    for item in ["minecraft:dirt", "minecraft:grass_block"] {
        send(
            &mut world,
            Mutation::PlayerCommand(PlayerCommand::Give {
                item: item.into(),
                count: 5,
            }),
        );
    }
    let aimed = send(
        &mut world,
        Mutation::ControlPlayer(PlayerInput {
            pitch: -0.95,
            ..PlayerInput::default()
        }),
    )
    .snapshot;
    let ground = aimed.player.unwrap().target.unwrap();
    let pick = send(
        &mut world,
        Mutation::UseBlock {
            action: BlockAction::Pick,
            target: ground,
        },
    );
    let inventory = pick.snapshot.inventory.unwrap();
    assert_eq!(inventory.selected(), 1);
    assert_eq!(inventory.slots()[1].unwrap().count, 5);
    let selected = send(
        &mut world,
        Mutation::EditInventory {
            action: InventoryAction::Select { slot: 0 },
            expected_revision: inventory.revision(),
        },
    );
    let target = selected.snapshot.player.unwrap().target.unwrap();
    let placed = target.placement.unwrap().position;
    assert_eq!(placed.y, -60);
    let result = send(
        &mut world,
        Mutation::UseBlock {
            action: BlockAction::Place,
            target,
        },
    );
    assert_eq!(
        result.snapshot.inventory.unwrap().slots()[0].unwrap().count,
        4
    );
    let target = result.snapshot.player.unwrap().target.unwrap();
    assert_eq!(target.position, placed);
    let result = send(
        &mut world,
        Mutation::UseBlock {
            action: BlockAction::Break,
            target,
        },
    );
    assert_eq!(result.acks[0].result, Ok(Effect::MiningStarted));
    send(&mut world, Mutation::CancelMining);
    for _ in 0..15 {
        world.step(Vec::new());
    }
    assert_eq!(
        world.snapshot().chunks[&placed.chunk()].block(placed),
        world.registry().block_id("rustcraft_java:dirt")
    );
    send(
        &mut world,
        Mutation::UseBlock {
            action: BlockAction::Break,
            target,
        },
    );
    for _ in 0..14 {
        world.step(Vec::new());
    }
    assert_eq!(
        world.snapshot().chunks[&placed.chunk()].block(placed),
        world.registry().block_id("rustcraft:empty")
    );
    assert_eq!(
        world.snapshot().inventory.unwrap().slots()[0]
            .unwrap()
            .count,
        5
    );
}

#[test]
fn local_operator_can_supply_a_stack_split_it_and_teleport_to_a_new_chunk() {
    let preset = RulePreset::JavaFlatV1;
    let mut server = Server::playable_with_rules(7, preset.default_range(), preset).unwrap();
    let give = Mutation::PlayerCommand(PlayerCommand::Give {
        item: "minecraft:dirt".into(),
        count: 11,
    });
    let unprivileged = server.open_player_session().unwrap();
    let attempt = server.step(vec![Request {
        session: unprivileged,
        operation: 1,
        mutation: give.clone(),
    }]);
    assert_eq!(attempt.acks[0].result, Err(Rejection::MissingCapability));
    server.close_session(unprivileged);
    let session = server.open_local_operator().unwrap();
    let mut sequence = 0;
    let mut send = |server: &mut Server, mutation| {
        sequence += 1;
        let report = server.step(vec![Request {
            session,
            operation: sequence,
            mutation,
        }]);
        assert!(report.acks[0].result.is_ok(), "{:?}", report.acks[0]);
        report.snapshot
    };
    let snapshot = send(&mut server, give);
    let first = snapshot.inventory.unwrap();
    assert_eq!(first.slots().len(), 36);
    assert_eq!(first.hotbar().len(), 9);
    let split = send(
        &mut server,
        Mutation::EditInventory {
            action: InventoryAction::Click {
                slot: 0,
                button: InventoryButton::Right,
            },
            expected_revision: first.revision(),
        },
    )
    .inventory
    .unwrap();
    assert_eq!(split.carried().unwrap().count, 6);
    assert_eq!(split.slots()[0].unwrap().count, 5);
    let placed = send(
        &mut server,
        Mutation::EditInventory {
            action: InventoryAction::Click {
                slot: 9,
                button: InventoryButton::Right,
            },
            expected_revision: split.revision(),
        },
    )
    .inventory
    .unwrap();
    assert_eq!(placed.carried().unwrap().count, 5);
    assert_eq!(placed.slots()[9].unwrap().count, 1);
    let teleported = send(
        &mut server,
        Mutation::PlayerCommand(PlayerCommand::Teleport([
            Coordinate::Absolute(-32.0),
            Coordinate::Absolute(-60.0),
            Coordinate::Relative(64.0),
        ])),
    );
    assert_eq!(teleported.player.unwrap().position, [-32.0, -60.0, 64.5]);
    let checkpoint = server.checkpoint().unwrap();
    let restored = Server::restore(checkpoint.clone()).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), checkpoint);
    assert_eq!(restored.snapshot().inventory, teleported.inventory);
}

#[test]
fn creative_catalog_and_all_three_requested_blocks_work_without_item_changes() {
    let preset = RulePreset::JavaFlatV1;
    let mut world = Server::playable_with_rules(42, preset.default_range(), preset).unwrap();
    assert_eq!(world.configure_game_mode(None), GameMode::Creative);
    let session = world.open_local_operator().unwrap();
    let mut operation = 0;
    let mut send = |world: &mut Server, mutation| {
        operation += 1;
        let report = world.step(vec![Request {
            session,
            operation,
            mutation,
        }]);
        assert!(report.acks[0].result.is_ok(), "{:?}", report.acks[0]);
        report.snapshot
    };
    send(
        &mut world,
        Mutation::ControlPlayer(PlayerInput {
            pitch: -0.95,
            ..PlayerInput::default()
        }),
    );
    for block in ["smooth_stone", "dirt", "redstone"] {
        let block = world
            .registry()
            .block_id(&format!("rustcraft_java:{block}"))
            .unwrap();
        let revision = world.snapshot().inventory.unwrap().revision();
        let taken = send(
            &mut world,
            Mutation::EditInventory {
                action: InventoryAction::Catalog {
                    block,
                    stack: false,
                },
                expected_revision: revision,
            },
        )
        .inventory
        .unwrap();
        assert_eq!(taken.carried().unwrap().count, 1);
        let stack = send(
            &mut world,
            Mutation::EditInventory {
                action: InventoryAction::Catalog { block, stack: true },
                expected_revision: taken.revision(),
            },
        )
        .inventory
        .unwrap();
        assert_eq!(stack.carried().unwrap().count, 64);
        let placed_one = send(
            &mut world,
            Mutation::EditInventory {
                action: InventoryAction::Click {
                    slot: 0,
                    button: InventoryButton::Right,
                },
                expected_revision: stack.revision(),
            },
        )
        .inventory
        .unwrap();
        assert_eq!(placed_one.slots()[0].unwrap().count, 1);
        assert_eq!(placed_one.carried().unwrap().count, 63);
        let ready = send(
            &mut world,
            Mutation::EditInventory {
                action: InventoryAction::DiscardCarried,
                expected_revision: placed_one.revision(),
            },
        );
        let inventory = ready.inventory.unwrap();
        assert!(inventory.carried().is_none());
        let target = ready.player.unwrap().target.unwrap();
        let position = target.placement.unwrap().position;
        let result = send(
            &mut world,
            Mutation::UseBlock {
                action: BlockAction::Place,
                target,
            },
        );
        assert_eq!(
            result.chunks[&position.chunk()].block(position),
            Some(block)
        );
        assert_eq!(result.inventory.as_ref(), Some(&inventory));
        let target = result.player.unwrap().target.unwrap();
        assert_eq!(target.position, position);
        let result = send(
            &mut world,
            Mutation::UseBlock {
                action: BlockAction::Break,
                target,
            },
        );
        assert_eq!(
            result.chunks[&position.chunk()].block(position),
            world.registry().block_id("rustcraft:empty")
        );
        assert_eq!(result.inventory.as_ref(), Some(&inventory));

        let revision = result.inventory.unwrap().revision();
        let taken = send(
            &mut world,
            Mutation::EditInventory {
                action: InventoryAction::Click {
                    slot: 0,
                    button: InventoryButton::Left,
                },
                expected_revision: revision,
            },
        )
        .inventory
        .unwrap();
        send(
            &mut world,
            Mutation::EditInventory {
                action: InventoryAction::DiscardCarried,
                expected_revision: taken.revision(),
            },
        );
    }

    let survival = send(
        &mut world,
        Mutation::PlayerCommand(PlayerCommand::GameMode(GameMode::Survival)),
    );
    assert_eq!(survival.game_mode, GameMode::Survival);
    assert_eq!(
        Server::restore(world.checkpoint().unwrap())
            .unwrap()
            .game_mode(),
        GameMode::Survival
    );
}
