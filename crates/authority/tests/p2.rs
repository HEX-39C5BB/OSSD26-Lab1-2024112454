use std::f64::consts::{FRAC_PI_2, PI};

use rustcraft_authority::{
    BlockAction, BlockTarget, Capabilities, ComponentValue, Effect, InventoryAction, Mutation,
    PlayerInput, Rejection, Request, Server, SessionId, Snapshot, SoundKind, TickReport,
    ViewDistance, WorldSound,
};
use rustcraft_world::{BlockPos, BlockStateId, ChunkPos};

fn block(snapshot: &Snapshot, position: BlockPos) -> BlockStateId {
    snapshot.chunks[&position.chunk()].block(position).unwrap()
}

struct Play {
    server: Server,
    session: SessionId,
    operation: u64,
}

impl Play {
    fn new(seed: u64) -> Self {
        let mut server = Server::playable(seed).unwrap();
        let session = server.open_player_session().unwrap();
        Self {
            server,
            session,
            operation: 0,
        }
    }

    fn send(&mut self, mutation: Mutation) -> TickReport {
        self.operation += 1;
        self.server.step(vec![Request {
            session: self.session,
            operation: self.operation,
            mutation,
        }])
    }

    fn idle(&mut self, ticks: usize) -> Snapshot {
        for _ in 0..ticks {
            self.server.step(vec![]);
        }
        self.server.snapshot()
    }

    fn put(&mut self, position: BlockPos, name: &str) {
        let admin = self.server.open_local_session(Capabilities::P1_HOST);
        let revision = self.server.snapshot().chunks[&position.chunk()].revision();
        let state = self.server.registry().block_id(name).unwrap();
        let report = self.server.step(vec![Request {
            session: admin,
            operation: 1,
            mutation: Mutation::SetBlock {
                position,
                state,
                expected_revision: revision,
            },
        }]);
        assert_eq!(
            report.acks[0].result,
            Ok(Effect::ChunkChanged(position.chunk()))
        );
        self.server.close_session(admin);
    }
}

#[test]
fn generates_seeded_layers_across_negative_and_positive_chunk_boundaries() {
    let play = Play::new(42);
    let first = play.server.snapshot();
    let same_seed = Play::new(42).server.snapshot();
    let other_seed = Play::new(43).server.snapshot();
    assert_eq!(first, same_seed);
    assert_ne!(first.chunks, other_seed.chunks);
    assert_eq!(first.chunks.len(), 9);
    for (y, name) in [
        (0, "rustcraft:empty"),
        (-1, "rustcraft_content:lichenstone"),
        (-2, "rustcraft_content:loam"),
        (-17, "rustcraft_content:basalt"),
    ] {
        let state = play.server.registry().block_id(name).unwrap();
        for (x, z) in [(-1, -1), (0, 0), (15, 15), (16, 16)] {
            assert_eq!(block(&first, BlockPos::new(x, y, z)), state);
        }
    }
}

#[test]
fn falls_jumps_and_collides_with_walls_and_unloaded_chunks() {
    let mut play = Play::new(42);
    assert_eq!(play.server.snapshot().player.unwrap().position[1], 2.0);
    let landed = play.idle(16).player.unwrap();
    assert!(landed.grounded);
    assert!(landed.position[1].abs() < 1e-8);

    play.put(BlockPos::new(0, 0, 2), "rustcraft_content:basalt");
    play.send(Mutation::ControlPlayer(PlayerInput {
        forward: 1,
        ..PlayerInput::default()
    }));
    let stopped = play.idle(20).player.unwrap();
    assert!((stopped.position[2] - 1.7).abs() < 1e-8);
    assert!(stopped.grounded);

    let jump = PlayerInput {
        jump: true,
        ..PlayerInput::default()
    };
    let airborne = play
        .send(Mutation::ControlPlayer(jump))
        .snapshot
        .player
        .unwrap();
    assert!(!airborne.grounded && airborne.position[1] > 0.0);
    let landed_again = play.idle(40).player.unwrap();
    assert!(landed_again.grounded && landed_again.position[1].abs() < 1e-8);

    play.send(Mutation::ControlPlayer(PlayerInput {
        forward: 1,
        yaw: PI,
        ..PlayerInput::default()
    }));
    let edge = play.idle(110).player.unwrap();
    assert!((edge.position[2] + 15.7).abs() < 1e-8, "{edge:?}");
    assert!(edge.grounded);
}

#[test]
fn enters_flight_on_two_presses_and_walks_again_after_descending() {
    let mut play = Play::new(42);
    play.idle(16);
    let jump = PlayerInput {
        jump: true,
        ..PlayerInput::default()
    };
    assert!(
        !play
            .send(Mutation::ControlPlayer(jump))
            .snapshot
            .player
            .unwrap()
            .flying
    );
    assert!(
        !play
            .send(Mutation::ControlPlayer(jump))
            .snapshot
            .player
            .unwrap()
            .flying
    );
    play.send(Mutation::ControlPlayer(PlayerInput::default()));
    let takeoff = play
        .send(Mutation::ControlPlayer(jump))
        .snapshot
        .player
        .unwrap();
    assert!(takeoff.flying && !takeoff.grounded);
    let climbed = play.idle(6).player.unwrap();
    assert!(climbed.position[1] > takeoff.position[1]);

    play.send(Mutation::ControlPlayer(PlayerInput::default()));
    assert_eq!(
        play.idle(10).player.unwrap().position[1],
        climbed.position[1]
    );
    play.send(Mutation::ControlPlayer(PlayerInput {
        descend: true,
        ..PlayerInput::default()
    }));
    let landed = play.idle(20).player.unwrap();
    assert!(!landed.flying && landed.grounded);
    assert!(landed.position[1].abs() < 1e-9);

    let ground_step = play
        .send(Mutation::ControlPlayer(PlayerInput {
            forward: 1,
            ..PlayerInput::default()
        }))
        .snapshot
        .player
        .unwrap();
    assert!(
        ground_step.grounded && !ground_step.flying && ground_step.position[2] > landed.position[2]
    );
}

#[test]
fn cancels_flight_on_two_presses_and_applies_gravity_immediately() {
    let mut play = Play::new(42);
    play.idle(16);
    play.send(Mutation::ControlPlayer(PlayerInput {
        jump_presses: 2,
        ..PlayerInput::default()
    }));
    let jump = PlayerInput {
        jump: true,
        ..PlayerInput::default()
    };
    let first = play
        .send(Mutation::ControlPlayer(jump))
        .snapshot
        .player
        .unwrap();
    let held = play.idle(2).player.unwrap();
    assert!(first.flying && held.flying && held.position[1] > first.position[1]);
    play.send(Mutation::ControlPlayer(PlayerInput::default()));
    let cancelled = play
        .send(Mutation::ControlPlayer(jump))
        .snapshot
        .player
        .unwrap();
    assert!(!cancelled.flying && !cancelled.grounded);
    assert!(cancelled.position[1] < held.position[1]);
    let landed = play.idle(20).player.unwrap();
    assert!(landed.grounded && !landed.flying);
    assert!(landed.position[1].abs() < 1e-9);
}

#[test]
fn clears_the_flight_tap_sequence_on_automatic_landing() {
    let mut play = Play::new(42);
    play.idle(16);
    play.send(Mutation::ControlPlayer(PlayerInput {
        jump_presses: 2,
        ..PlayerInput::default()
    }));
    let jump = PlayerInput {
        jump: true,
        ..PlayerInput::default()
    };
    play.send(Mutation::ControlPlayer(jump));
    play.send(Mutation::ControlPlayer(PlayerInput {
        descend: true,
        ..PlayerInput::default()
    }));
    let landed = play.idle(1).player.unwrap();
    assert!(landed.grounded && !landed.flying);
    let jumping = play
        .send(Mutation::ControlPlayer(jump))
        .snapshot
        .player
        .unwrap();
    assert!(!jumping.grounded && !jumping.flying && jumping.position[1] > 0.0);
}

#[test]
fn keeps_ordinary_jumps_when_presses_are_too_far_apart() {
    let mut play = Play::new(42);
    play.idle(16);
    let jump = PlayerInput {
        jump: true,
        ..PlayerInput::default()
    };
    play.send(Mutation::ControlPlayer(jump));
    play.send(Mutation::ControlPlayer(PlayerInput::default()));
    play.idle(8);
    assert!(
        !play
            .send(Mutation::ControlPlayer(jump))
            .snapshot
            .player
            .unwrap()
            .flying
    );
}

fn build_and_break() -> Vec<TickReport> {
    let mut play = Play::new(42);
    play.idle(16);
    let aimed = play.send(Mutation::ControlPlayer(PlayerInput {
        pitch: -0.65,
        ..PlayerInput::default()
    }));
    let target = aimed.snapshot.player.unwrap().target.unwrap();
    let placement = target.placement.unwrap().position;
    let place = Mutation::UseBlock {
        action: BlockAction::Place,
        target,
    };
    let placed = play.send(place.clone());
    assert_eq!(
        placed.acks[0].result,
        Ok(Effect::ChunkChanged(placement.chunk()))
    );
    assert_eq!(
        placed.sounds,
        [WorldSound {
            kind: SoundKind::Place,
            position: placement
        }]
    );
    assert_eq!(
        block(&placed.snapshot, placement),
        play.server
            .registry()
            .block_id("rustcraft_content:basalt")
            .unwrap()
    );
    let duplicate = play.server.step(vec![Request {
        session: play.session,
        operation: play.operation,
        mutation: place.clone(),
    }]);
    assert_eq!(duplicate.acks[0].result, Err(Rejection::DuplicateOperation));
    assert!(duplicate.sounds.is_empty());
    let stale = play.send(place);
    assert_eq!(stale.acks[0].result, Err(Rejection::StaleBlockTarget));
    assert!(stale.sounds.is_empty());
    let target = stale.snapshot.player.unwrap().target.unwrap();
    assert_eq!(target.position, placement);
    let broken = play.send(Mutation::UseBlock {
        action: BlockAction::Break,
        target,
    });
    assert_eq!(
        broken.acks[0].result,
        Ok(Effect::ChunkChanged(placement.chunk()))
    );
    assert_eq!(
        broken.sounds,
        [WorldSound {
            kind: SoundKind::Break,
            position: placement
        }]
    );
    assert_eq!(
        block(&broken.snapshot, placement),
        play.server.registry().block_id("rustcraft:empty").unwrap()
    );
    assert_ne!(
        block(&placed.snapshot, placement),
        block(&broken.snapshot, placement)
    );
    vec![aimed, placed, duplicate, stale, broken]
}

#[test]
fn replays_place_and_break_with_identical_authoritative_results() {
    assert_eq!(build_and_break(), build_and_break());
}

#[test]
fn player_and_host_sessions_cannot_submit_each_others_mutations() {
    let mut play = Play::new(42);
    play.idle(16);
    play.send(Mutation::ControlPlayer(PlayerInput {
        pitch: -0.65,
        ..PlayerInput::default()
    }));
    let host = play.server.open_local_session(Capabilities::P1_HOST);
    let position = BlockPos::new(0, 0, 4);
    let spawned = play.server.step(vec![Request {
        session: host,
        operation: 1,
        mutation: Mutation::SpawnEntity { position },
    }]);
    let Ok(Effect::Spawned(entity)) = spawned.acks[0].result else {
        panic!("host could not spawn the test entity");
    };
    let mut before = spawned.snapshot;
    let expected_revision = before
        .entities
        .iter()
        .find(|entry| entry.id == entity)
        .unwrap()
        .revision;
    let state = play
        .server
        .registry()
        .block_id("rustcraft_content:basalt")
        .unwrap();
    let player_mutations = [
        Mutation::SetBlock {
            position,
            state,
            expected_revision: before.chunks[&position.chunk()].revision(),
        },
        Mutation::SetChunkActive {
            position: ChunkPos { x: 2, z: 0 },
            active: true,
        },
        Mutation::SpawnEntity { position },
        Mutation::SetComponent {
            entity,
            expected_revision,
            name: "rustcraft_content:marker".into(),
            value: ComponentValue {
                version: 1,
                fields: vec![1],
            },
        },
        Mutation::DespawnEntity {
            entity,
            expected_revision,
        },
    ];
    let host_mutations = [
        Mutation::ControlPlayer(PlayerInput {
            forward: 1,
            ..PlayerInput::default()
        }),
        Mutation::UseBlock {
            action: BlockAction::Place,
            target: before.player.unwrap().target.unwrap(),
        },
        Mutation::EditInventory {
            action: InventoryAction::Select { slot: 1 },
            expected_revision: before.inventory.as_ref().unwrap().revision(),
        },
    ];
    let mut requests = Vec::new();
    for (session, mutations) in [
        (play.session, player_mutations.as_slice()),
        (host, host_mutations.as_slice()),
    ] {
        requests.extend(
            mutations
                .iter()
                .cloned()
                .zip(2..)
                .map(|(mutation, operation)| Request {
                    session,
                    operation,
                    mutation,
                }),
        );
    }
    let expected_acks = requests.len();
    let denied = play.server.step(requests);
    assert_eq!(denied.acks.len(), expected_acks);
    assert!(
        denied
            .acks
            .iter()
            .all(|ack| ack.result == Err(Rejection::MissingCapability)),
        "{:?}",
        denied.acks
    );
    assert!(denied.sounds.is_empty());
    before.tick += 1;
    assert_eq!(denied.snapshot, before);

    let reused = play.server.step(vec![Request {
        session: play.session,
        operation: 2,
        mutation: Mutation::ControlPlayer(PlayerInput::default()),
    }]);
    assert_eq!(reused.acks[0].result, Err(Rejection::DuplicateOperation));
}

#[test]
fn footsteps_follow_grounded_distance_and_stop_against_a_wall() {
    let mut play = Play::new(42);
    play.idle(16);
    play.put(BlockPos::new(0, 0, 4), "rustcraft_content:basalt");
    let first = play.send(Mutation::ControlPlayer(PlayerInput {
        forward: 1,
        ..PlayerInput::default()
    }));
    assert!(first.sounds.is_empty());
    let reports: Vec<_> = (0..29).map(|_| play.server.step(vec![])).collect();
    let footsteps: Vec<_> = reports.iter().flat_map(|report| &report.sounds).collect();
    assert_eq!(footsteps.len(), 2);
    assert!(footsteps.iter().all(|sound| sound.kind == SoundKind::Step));
    assert!(footsteps[0].position.z < footsteps[1].position.z);
    assert!(reports[18..].iter().all(|report| report.sounds.is_empty()));

    let jump = play.send(Mutation::ControlPlayer(PlayerInput {
        forward: 1,
        yaw: PI,
        jump: true,
        ..PlayerInput::default()
    }));
    assert!(!jump.snapshot.player.unwrap().grounded);
    assert!(jump.sounds.is_empty());
    for _ in 0..5 {
        let airborne = play.server.step(vec![]);
        assert!(!airborne.snapshot.player.unwrap().grounded);
        assert!(airborne.sounds.is_empty());
    }
}

#[test]
fn rejects_obstructed_distant_overlapping_and_unauthorized_edits() {
    let mut play = Play::new(42);
    play.idle(16);
    let far = BlockPos::new(0, 1, 7);
    let wall = BlockPos::new(0, 1, 2);
    play.put(far, "rustcraft_content:basalt");
    play.put(wall, "rustcraft_content:basalt");
    let snapshot = play.server.snapshot();
    assert_eq!(snapshot.player.unwrap().target.unwrap().position, wall);

    let target = BlockTarget {
        position: far,
        revision: snapshot.chunks[&ChunkPos::ORIGIN].revision(),
        placement: None,
    };
    let edit = Mutation::UseBlock {
        action: BlockAction::Break,
        target,
    };
    assert_eq!(
        play.send(edit.clone()).acks[0].result,
        Err(Rejection::StaleBlockTarget)
    );
    play.put(wall, "rustcraft:empty");
    assert_eq!(
        play.send(edit).acks[0].result,
        Err(Rejection::NoBlockTarget)
    );

    let aimed = play.send(Mutation::ControlPlayer(PlayerInput {
        pitch: -FRAC_PI_2,
        ..PlayerInput::default()
    }));
    let target = aimed.snapshot.player.unwrap().target.unwrap();
    assert_eq!(target.placement.unwrap().position, BlockPos::new(0, 0, 0));
    let blocked = play.send(Mutation::UseBlock {
        action: BlockAction::Place,
        target,
    });
    assert_eq!(blocked.acks[0].result, Err(Rejection::PlacementBlocked));

    let bypass = Mutation::SetBlock {
        position: far,
        state: play.server.registry().block_id("rustcraft:empty").unwrap(),
        expected_revision: blocked.snapshot.chunks[&ChunkPos::ORIGIN].revision(),
    };
    assert_eq!(
        play.send(bypass).acks[0].result,
        Err(Rejection::MissingCapability)
    );
    let input = Mutation::ControlPlayer(PlayerInput {
        yaw: f64::NAN,
        ..PlayerInput::default()
    });
    assert_eq!(
        play.send(input).acks[0].result,
        Err(Rejection::InvalidPlayerInput)
    );
}

#[test]
fn revokes_player_control_and_clears_held_movement_on_session_close() {
    let mut play = Play::new(42);
    assert!(play.server.open_player_session().is_err());
    play.idle(16);
    let movement = Mutation::ControlPlayer(PlayerInput {
        forward: 1,
        ..PlayerInput::default()
    });
    let position = play
        .send(movement.clone())
        .snapshot
        .player
        .unwrap()
        .position;
    play.server.close_session(play.session);
    assert_eq!(
        play.send(movement).acks[0].result,
        Err(Rejection::UnknownSession)
    );
    assert_eq!(play.idle(5).player.unwrap().position, position);
    assert!(play.server.open_player_session().is_ok());
}

#[test]
fn checks_both_chunk_revisions_for_placement_across_a_boundary() {
    let mut play = Play::new(42);
    play.idle(16);
    play.send(Mutation::ControlPlayer(PlayerInput {
        forward: 1,
        ..PlayerInput::default()
    }));
    play.idle(6);
    play.put(BlockPos::new(0, 1, -1), "rustcraft_content:basalt");
    let aimed = play.send(Mutation::ControlPlayer(PlayerInput {
        yaw: PI,
        ..PlayerInput::default()
    }));
    let target = aimed.snapshot.player.unwrap().target.unwrap();
    let placement = target.placement.unwrap().position;
    assert_eq!(target.position, BlockPos::new(0, 1, -1));
    assert_eq!(placement, BlockPos::new(0, 1, 0));
    assert_ne!(target.position.chunk(), placement.chunk());

    play.put(BlockPos::new(4, 0, 4), "rustcraft_content:basalt");
    let stale = play.send(Mutation::UseBlock {
        action: BlockAction::Place,
        target,
    });
    assert_eq!(stale.acks[0].result, Err(Rejection::StaleBlockTarget));
    let target = stale.snapshot.player.unwrap().target.unwrap();
    let placed = play.send(Mutation::UseBlock {
        action: BlockAction::Place,
        target,
    });
    assert_eq!(
        placed.acks[0].result,
        Ok(Effect::ChunkChanged(placement.chunk()))
    );
}

#[test]
fn keeps_chunk_active_while_it_contains_the_local_player() {
    let mut play = Play::new(42);
    let admin = play.server.open_local_session(Capabilities::P1_HOST);
    let report = play.server.step(vec![Request {
        session: admin,
        operation: 1,
        mutation: Mutation::SetChunkActive {
            position: ChunkPos::ORIGIN,
            active: false,
        },
    }]);
    assert_eq!(report.acks[0].result, Err(Rejection::ChunkOccupied));
    assert!(report.snapshot.chunks.contains_key(&ChunkPos::ORIGIN));
}

#[test]
fn fills_the_view_nearest_first_and_discards_pending_work_after_shrinking() {
    let mut play = Play::new(42);
    let initial = play.server.snapshot();
    play.server
        .set_local_view_distance(ViewDistance::from_radius(4).unwrap());
    let first = play.idle(1);
    let new: Vec<_> = first
        .chunks
        .keys()
        .filter(|pos| !initial.chunks.contains_key(pos))
        .copied()
        .collect();
    assert_eq!(new, [ChunkPos { x: -2, z: 0 }, ChunkPos { x: 0, z: -2 }]);
    assert_eq!(play.idle(7).chunks.len(), 25);
    play.server
        .set_local_view_distance(ViewDistance::from_radius(1).unwrap());
    assert_eq!(play.idle(1).chunks.len(), 9);
    assert_eq!(
        play.idle(40).chunks.keys().collect::<Vec<_>>(),
        initial.chunks.keys().collect::<Vec<_>>()
    );
}

#[test]
fn follows_the_player_across_a_negative_boundary_and_recovers_retired_edits() {
    let mut play = Play::new(42);
    play.server
        .set_local_view_distance(ViewDistance::from_radius(4).unwrap());
    play.idle(40);
    let edited = BlockPos::new(1, 0, 65);
    play.put(edited, "rustcraft_content:basalt");
    let original = block(&play.server.snapshot(), edited);
    play.send(Mutation::ControlPlayer(PlayerInput {
        forward: -1,
        ..PlayerInput::default()
    }));
    let moved = play.idle(33);
    assert!(moved.player.unwrap().position[2] < 0.0);
    assert_eq!(moved.chunks.len(), 81);
    assert!(moved.chunks.contains_key(&ChunkPos { x: 0, z: -5 }));
    assert!(!moved.chunks.contains_key(&edited.chunk()));

    let checkpoint = play.server.checkpoint().unwrap();
    assert!(
        checkpoint
            .chunks
            .iter()
            .any(|chunk| chunk.position == edited.chunk() && !chunk.active)
    );
    let mut server = Server::restore(checkpoint).unwrap();
    server.set_local_view_distance(ViewDistance::from_radius(4).unwrap());
    let session = server.open_player_session().unwrap();
    server.step(vec![Request {
        session,
        operation: 1,
        mutation: Mutation::ControlPlayer(PlayerInput {
            forward: 1,
            ..PlayerInput::default()
        }),
    }]);
    for _ in 0..38 {
        server.step(vec![]);
    }
    let returned = server.snapshot();
    assert!(returned.player.unwrap().position[2] > 0.0);
    assert_eq!(returned.chunks.len(), 81);
    assert_eq!(block(&returned, edited), original);
}

#[test]
fn rejects_stale_inventory_operations_and_placement_from_an_empty_slot() {
    use rustcraft_authority::InventoryAction;

    let mut play = Play::new(42);
    play.idle(16);
    let inventory = play.server.snapshot().inventory.unwrap();
    let select = play.send(Mutation::EditInventory {
        action: InventoryAction::Select { slot: 7 },
        expected_revision: inventory.revision(),
    });
    assert_eq!(select.acks[0].result, Ok(Effect::InventoryChanged));
    let stale = play.send(Mutation::EditInventory {
        action: InventoryAction::Transfer {
            from: 0,
            to: 7,
            count: 1,
        },
        expected_revision: inventory.revision(),
    });
    assert!(matches!(
        stale.acks[0].result,
        Err(Rejection::StaleInventory { .. })
    ));
    let aimed = play.send(Mutation::ControlPlayer(PlayerInput {
        pitch: -0.65,
        ..PlayerInput::default()
    }));
    let target = aimed.snapshot.player.unwrap().target.unwrap();
    let place = play.send(Mutation::UseBlock {
        action: BlockAction::Place,
        target,
    });
    assert_eq!(place.acks[0].result, Err(Rejection::EmptySelectedSlot));
    assert_eq!(place.snapshot.chunks, aimed.snapshot.chunks);
    assert_eq!(place.snapshot.inventory, aimed.snapshot.inventory);
}
