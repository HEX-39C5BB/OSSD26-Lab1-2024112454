use std::f64::consts::PI;

use rustcraft_authority::{
    BlockAction, Effect, InventoryAction, Mutation, PlayerInput, Rejection, Request, Server,
    SessionId, SoundKind, TickReport, ViewDistance, WorldSound,
};
use rustcraft_persistence::{load_existing, save};
use rustcraft_world::{ChunkPos, SECTION_VOLUME};

struct Replay {
    continuous: Server,
    restored: Server,
    continuous_session: SessionId,
    restored_session: SessionId,
    operation: u64,
}

impl Replay {
    fn step(&mut self, mutation: Option<Mutation>) -> TickReport {
        self.operation += u64::from(mutation.is_some());
        let requests = |session| {
            mutation
                .iter()
                .cloned()
                .map(|mutation| Request {
                    session,
                    operation: self.operation,
                    mutation,
                })
                .collect()
        };
        let expected = self.continuous.step(requests(self.continuous_session));
        let mut actual = self.restored.step(requests(self.restored_session));
        let tick = expected.snapshot.tick;
        for ack in &mut actual.acks {
            assert_eq!(ack.session, self.restored_session, "session at tick {tick}");
            ack.session = self.continuous_session;
        }
        assert_eq!(
            actual.acks, expected.acks,
            "acknowledgements at tick {tick}"
        );
        assert_eq!(
            actual.sounds, expected.sounds,
            "world sounds at tick {tick}"
        );
        assert_eq!(
            actual.due_tasks, expected.due_tasks,
            "scheduled tasks at tick {tick}"
        );
        assert_eq!(
            actual.snapshot.player, expected.snapshot.player,
            "player at tick {tick}"
        );
        assert_eq!(
            actual.snapshot.inventory, expected.snapshot.inventory,
            "inventory at tick {tick}"
        );
        assert_eq!(actual.snapshot.tick, tick);
        assert_eq!(
            actual.snapshot.entities, expected.snapshot.entities,
            "entities at tick {tick}"
        );
        assert!(
            actual
                .snapshot
                .chunks
                .keys()
                .eq(expected.snapshot.chunks.keys()),
            "active chunks at tick {tick}"
        );
        for (position, wanted) in &expected.snapshot.chunks {
            let loaded = &actual.snapshot.chunks[position];
            assert_eq!(
                loaded.revision(),
                wanted.revision(),
                "chunk {position:?} at tick {tick}"
            );
            assert!(
                loaded.sections().keys().eq(wanted.sections().keys()),
                "sections in {position:?} at tick {tick}"
            );
            for (y, section) in wanted.sections() {
                let loaded = &loaded.sections()[y];
                // Saved palettes are rebuilt in voxel order. Compare blocks when their packing differs.
                if loaded != section {
                    for index in 0..SECTION_VOLUME {
                        assert_eq!(
                            loaded.get(index),
                            section.get(index),
                            "voxel {index} in {position:?} section {y} at tick {tick}"
                        );
                    }
                }
            }
        }
        expected
    }

    fn walk(&mut self, yaw: f64, ticks: usize) -> TickReport {
        let mut report = self.step(Some(Mutation::ControlPlayer(PlayerInput {
            forward: 1,
            yaw,
            ..PlayerInput::default()
        })));
        assert_eq!(report.acks[0].result, Ok(Effect::PlayerInputAccepted));
        for _ in 1..ticks {
            report = self.step(None);
        }
        report
    }
}

fn submit(
    server: &mut Server,
    session: SessionId,
    operation: u64,
    mutation: Mutation,
) -> TickReport {
    server.step(vec![Request {
        session,
        operation,
        mutation,
    }])
}

#[test]
fn replays_movement_and_block_inventory_transactions_after_restoring_a_checkpoint() {
    let root = tempfile::tempdir().unwrap();
    let distance = ViewDistance::from_radius(1).unwrap();
    let aim = PlayerInput {
        yaw: PI,
        pitch: -0.65,
        ..PlayerInput::default()
    };
    let mut continuous = Server::playable(42).unwrap();
    continuous.set_local_view_distance(distance);
    let session = continuous.open_player_session().unwrap();
    for _ in 0..16 {
        continuous.step(vec![]);
    }

    let aimed = submit(&mut continuous, session, 1, Mutation::ControlPlayer(aim));
    let inventory = aimed.snapshot.inventory.unwrap();
    let moved = submit(
        &mut continuous,
        session,
        2,
        Mutation::EditInventory {
            action: InventoryAction::Transfer {
                from: 1,
                to: 4,
                count: 3,
            },
            expected_revision: inventory.revision(),
        },
    );
    assert_eq!(moved.acks[0].result, Ok(Effect::InventoryChanged));
    let selected = submit(
        &mut continuous,
        session,
        3,
        Mutation::EditInventory {
            action: InventoryAction::Select { slot: 4 },
            expected_revision: moved.snapshot.inventory.unwrap().revision(),
        },
    );
    assert_eq!(selected.acks[0].result, Ok(Effect::InventoryChanged));
    let target = selected.snapshot.player.unwrap().target.unwrap();
    let position = target.placement.unwrap().position;
    let chunk = position.chunk();
    let empty = selected.snapshot.chunks[&chunk].block(position);
    let placed = submit(
        &mut continuous,
        session,
        4,
        Mutation::UseBlock {
            action: BlockAction::Place,
            target,
        },
    );
    assert_eq!(placed.acks[0].result, Ok(Effect::ChunkChanged(chunk)));
    let placed_block = placed.snapshot.chunks[&chunk].block(position);
    assert_ne!(placed_block, empty);
    let placed_inventory = placed.snapshot.inventory.unwrap();
    assert_eq!(placed_inventory.slots()[4].unwrap().count, 2);

    // Closing a player session clears transient input on the continuous path too.
    continuous.close_session(session);
    let checkpoint = continuous.checkpoint().unwrap();
    save(root.path(), None, &continuous).unwrap();
    let (_, mut restored) = load_existing(root.path()).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), checkpoint);
    restored.set_local_view_distance(distance);
    let continuous_session = continuous.open_player_session().unwrap();
    let restored_session = restored.open_player_session().unwrap();
    let mut replay = Replay {
        continuous,
        restored,
        continuous_session,
        restored_session,
        operation: 0,
    };

    let outward = replay.walk(0.0, 140);
    let outward_chunk = outward.snapshot.player.unwrap().block_position().chunk();
    assert_eq!(outward_chunk, ChunkPos { x: 0, z: 2 });
    assert!(!outward.snapshot.chunks.contains_key(&chunk));
    let generated = ChunkPos { x: 0, z: 3 };
    assert!(outward.snapshot.chunks.contains_key(&generated));
    assert!(
        !checkpoint
            .chunks
            .iter()
            .any(|entry| entry.position == generated)
    );
    let returned = replay.walk(PI, 140);
    assert_eq!(
        returned.snapshot.player.unwrap().block_position().chunk(),
        ChunkPos::ORIGIN
    );
    assert_eq!(
        returned.snapshot.chunks[&chunk].block(position),
        placed_block
    );

    let aimed = replay.step(Some(Mutation::ControlPlayer(aim)));
    let target = aimed.snapshot.player.unwrap().target.unwrap();
    assert_eq!(target.position, position);
    let remove = Mutation::UseBlock {
        action: BlockAction::Break,
        target,
    };
    let broken = replay.step(Some(remove.clone()));
    assert_eq!(broken.acks[0].result, Ok(Effect::ChunkChanged(chunk)));
    assert_eq!(
        broken.sounds,
        [WorldSound {
            kind: SoundKind::Break,
            position
        }]
    );
    assert_eq!(broken.snapshot.chunks[&chunk].block(position), empty);
    let recovered = broken.snapshot.inventory.unwrap();
    assert_eq!(
        recovered.slots()[1].unwrap().count,
        placed_inventory.slots()[1].unwrap().count + 1
    );

    let stale = replay.step(Some(remove));
    assert_eq!(stale.acks[0].result, Err(Rejection::StaleBlockTarget));
    assert!(stale.sounds.is_empty());
    assert_eq!(stale.snapshot.inventory.as_ref().unwrap(), &recovered);
    assert_eq!(
        stale.snapshot.chunks[&chunk],
        broken.snapshot.chunks[&chunk]
    );

    let transferred = replay.step(Some(Mutation::EditInventory {
        action: InventoryAction::Transfer {
            from: 1,
            to: 4,
            count: 1,
        },
        expected_revision: recovered.revision(),
    }));
    assert_eq!(transferred.acks[0].result, Ok(Effect::InventoryChanged));
    let target = transferred.snapshot.player.unwrap().target.unwrap();
    assert_eq!(target.placement.unwrap().position, position);
    let replaced = replay.step(Some(Mutation::UseBlock {
        action: BlockAction::Place,
        target,
    }));
    assert_eq!(replaced.acks[0].result, Ok(Effect::ChunkChanged(chunk)));
    assert_eq!(
        replaced.sounds,
        [WorldSound {
            kind: SoundKind::Place,
            position
        }]
    );
    assert_eq!(
        replaced.snapshot.chunks[&chunk].block(position),
        placed_block
    );
    assert_eq!(
        replaced.snapshot.inventory.unwrap().slots(),
        placed_inventory.slots()
    );

    let expected = replay.continuous.checkpoint().unwrap();
    assert!(
        expected
            .chunks
            .iter()
            .any(|entry| entry.position == generated && !entry.active)
    );
    assert_eq!(replay.restored.checkpoint().unwrap(), expected);
}
