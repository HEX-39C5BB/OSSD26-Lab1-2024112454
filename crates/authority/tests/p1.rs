use rustcraft_authority::{
    Capabilities, ComponentValue, Effect, Mutation, Rejection, Request, Server, SessionId,
    TickReport,
};
use rustcraft_world::{BlockPos, ChunkPos};

fn request(session: SessionId, operation: u64, mutation: Mutation) -> Request {
    Request {
        session,
        operation,
        mutation,
    }
}

fn replay(reverse_arrival: bool) -> Vec<TickReport> {
    let mut server = Server::bootstrap(42).unwrap();
    let session = server.open_local_session(Capabilities::P1_HOST);
    let initial = server.snapshot();
    let state = server
        .registry()
        .block_id("rustcraft_content:basalt")
        .unwrap();
    let revision = initial.chunks[&ChunkPos::ORIGIN].revision();
    let position = BlockPos::new(0, -1, 0);
    let block = |position| Mutation::SetBlock {
        position,
        state,
        expected_revision: revision,
    };
    let mut requests = vec![
        request(session, 1, block(position)),
        request(session, 2, block(BlockPos::new(1, -1, 0))),
        request(session, 3, Mutation::SpawnEntity { position }),
    ];
    if reverse_arrival {
        requests.reverse();
    }
    let first = server.step(requests);
    assert_eq!(
        first.acks[0].result,
        Ok(Effect::ChunkChanged(ChunkPos::ORIGIN))
    );
    assert_eq!(
        first.acks[1].result,
        Err(Rejection::StaleChunk {
            actual: revision + 1
        })
    );
    let Ok(Effect::Spawned(entity)) = first.acks[2].result else {
        panic!("expected entity spawn");
    };
    assert_eq!(
        first.snapshot.chunks[&ChunkPos::ORIGIN].block(position),
        Some(state)
    );
    assert_ne!(
        initial.chunks[&ChunkPos::ORIGIN].block(position),
        Some(state)
    );
    let set_component = |version| Mutation::SetComponent {
        entity,
        expected_revision: first.snapshot.entities[0].revision,
        name: "rustcraft_content:marker".into(),
        value: ComponentValue {
            version,
            fields: vec![13],
        },
    };
    let second = server.step(vec![
        request(
            session,
            3,
            Mutation::DespawnEntity {
                entity,
                expected_revision: 1,
            },
        ),
        request(session, 4, set_component(2)),
        request(session, 5, set_component(1)),
        request(session, 6, set_component(1)),
    ]);
    assert_eq!(second.acks[0].result, Err(Rejection::DuplicateOperation));
    assert_eq!(second.acks[1].result, Err(Rejection::InvalidComponent));
    assert_eq!(second.acks[2].result, Ok(Effect::EntityChanged(entity)));
    assert_eq!(
        second.acks[3].result,
        Err(Rejection::StaleEntity { actual: 2 })
    );
    assert_eq!(
        second.snapshot.entities[0].components["rustcraft_content:marker"].fields,
        [13]
    );
    let third = server.step(vec![
        request(
            session,
            7,
            Mutation::DespawnEntity {
                entity,
                expected_revision: 2,
            },
        ),
        request(session, 8, Mutation::SpawnEntity { position }),
    ]);
    assert_ne!(third.snapshot.entities[0].id, entity);
    assert!(third.snapshot.entities[0].components.is_empty());
    vec![first, second, third]
}

#[test]
fn replays_tick_reports_independently_of_same_tick_arrival_order() {
    let reports = replay(false);
    assert_eq!(reports, replay(true));
    assert_eq!(
        reports
            .iter()
            .map(|report| report.snapshot.tick)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
}

#[test]
fn rejects_stale_chunk_version_after_deactivation_and_reactivation() {
    let mut server = Server::bootstrap(0).unwrap();
    let initial = server.snapshot();
    let session = server.open_local_session(Capabilities::P1_HOST);
    let state = server
        .registry()
        .block_id("rustcraft_content:basalt")
        .unwrap();
    let position = BlockPos::new(0, 0, 0);
    let set_active = |active| Mutation::SetChunkActive {
        position: ChunkPos::ORIGIN,
        active,
    };
    let edit = Mutation::SetBlock {
        position,
        state,
        expected_revision: initial.chunks[&ChunkPos::ORIGIN].revision(),
    };
    let inactive = server.step(vec![
        request(session, 1, set_active(false)),
        request(session, 2, edit.clone()),
    ]);
    assert!(inactive.snapshot.chunks.is_empty());
    assert_eq!(inactive.acks[1].result, Err(Rejection::InactiveChunk));
    let active = server.step(vec![
        request(session, 3, set_active(true)),
        request(session, 4, edit),
    ]);
    assert_eq!(
        active.acks[1].result,
        Err(Rejection::StaleChunk { actual: 3 })
    );
    assert_eq!(
        active.snapshot.chunks[&ChunkPos::ORIGIN].block(position),
        initial.chunks[&ChunkPos::ORIGIN].block(position)
    );
}

#[test]
fn rejects_duplicate_requests_and_checks_capabilities_and_session_lifetime() {
    let mut server = Server::bootstrap(0).unwrap();
    let session = server.open_local_session(Capabilities {
        edit_blocks: false,
        edit_entities: true,
        manage_chunks: false,
    });
    let position = BlockPos::new(0, 0, 0);
    let spawn = request(session, 1, Mutation::SpawnEntity { position });
    let acks = server.step(vec![spawn.clone(), spawn]).acks;
    assert_eq!(acks.len(), 2);
    assert!(
        acks.iter()
            .all(|ack| ack.result == Err(Rejection::DuplicateOperation))
    );
    let unload = request(
        session,
        2,
        Mutation::SetChunkActive {
            position: ChunkPos::ORIGIN,
            active: false,
        },
    );
    assert_eq!(
        server.step(vec![unload.clone()]).acks[0].result,
        Err(Rejection::MissingCapability)
    );
    server.close_session(session);
    assert_eq!(
        server.step(vec![unload]).acks[0].result,
        Err(Rejection::UnknownSession)
    );
    assert!(server.snapshot().entities.is_empty());
}

#[test]
fn host_grants_apply_only_to_their_own_mutation_category() {
    for (edit_blocks, manage_chunks, edit_entities) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let mut server = Server::bootstrap(42).unwrap();
        let session = server.open_local_session(Capabilities {
            edit_blocks,
            manage_chunks,
            edit_entities,
        });
        let position = BlockPos::new(0, 0, 0);
        let state = server
            .registry()
            .block_id("rustcraft_content:basalt")
            .unwrap();
        let expected_revision = server.snapshot().chunks[&ChunkPos::ORIGIN].revision();
        let neighbor = ChunkPos { x: 1, z: 0 };
        let report = server.step(vec![
            request(
                session,
                1,
                Mutation::SetBlock {
                    position,
                    state,
                    expected_revision,
                },
            ),
            request(
                session,
                2,
                Mutation::SetChunkActive {
                    position: neighbor,
                    active: true,
                },
            ),
            request(session, 3, Mutation::SpawnEntity { position }),
        ]);
        assert_eq!(
            report.acks[0].result,
            if edit_blocks {
                Ok(Effect::ChunkChanged(ChunkPos::ORIGIN))
            } else {
                Err(Rejection::MissingCapability)
            }
        );
        assert_eq!(
            report.acks[1].result,
            if manage_chunks {
                Ok(Effect::ChunkChanged(neighbor))
            } else {
                Err(Rejection::MissingCapability)
            }
        );
        if edit_entities {
            assert!(matches!(report.acks[2].result, Ok(Effect::Spawned(_))));
        } else {
            assert_eq!(report.acks[2].result, Err(Rejection::MissingCapability));
        }
    }
}
