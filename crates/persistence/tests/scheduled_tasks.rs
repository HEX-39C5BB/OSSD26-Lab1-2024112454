use std::fs;

use rustcraft_authority::{
    Capabilities, Effect, Mutation, Rejection, Request, ScheduledOperation, Server, TaskEvent,
};
use rustcraft_persistence::{load_existing, migrate_checkpoint, save};
use rustcraft_world::{BlockPos, ChunkPos};

#[test]
fn delayed_tasks_survive_save_and_migration_in_due_tick_and_id_order() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let migrated_world = root.path().join("migrated");
    let mut original = Server::playable(42).unwrap();
    let later = original.schedule_task(4).unwrap();
    let first = original.schedule_task(3).unwrap();
    let second = original.schedule_task(3).unwrap();
    let canceled = original.schedule_task(2).unwrap();
    assert!(original.cancel_task(canceled));
    assert!(!original.cancel_task(canceled));
    assert!(original.schedule_task(0).is_err());
    assert!(original.schedule_task(u64::MAX).is_err());
    assert!(original.step(vec![]).due_tasks.is_empty());

    assert_eq!(save(&world, None, &original).unwrap(), 1);
    assert_eq!(migrate_checkpoint(&world, &migrated_world).unwrap(), 1);
    let (_, restored) = load_existing(&world).unwrap();
    let (_, migrated) = load_existing(&migrated_world).unwrap();
    for mut server in [original, restored, migrated] {
        assert!(server.step(vec![]).due_tasks.is_empty());
        assert_eq!(server.step(vec![]).due_tasks, [first, second]);
        assert_eq!(server.step(vec![]).due_tasks, [later]);
        assert!(server.step(vec![]).due_tasks.is_empty());
        assert!(!server.cancel_task(first));

        let checkpoint = server.checkpoint().unwrap();
        assert!(checkpoint.scheduled_tasks.pending.is_empty());
        let mut reopened = Server::restore(checkpoint).unwrap();
        let next = reopened.schedule_task(1).unwrap();
        assert_eq!(next.id, canceled.id + 1);
        assert_eq!(reopened.step(vec![]).due_tasks, [next]);
    }
}

#[test]
fn rejects_invalid_pending_tasks_before_publishing_a_migration() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let destination = root.path().join("migrated");
    let mut server = Server::playable(42).unwrap();
    server.schedule_task(2).unwrap();
    server.schedule_task(3).unwrap();
    server.step(vec![]);
    save(&world, None, &server).unwrap();
    let path = world.join("checkpoints/1/manifest.json");
    let valid: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();

    for (field, value, diagnostic) in [
        (
            "/scheduled_tasks",
            serde_json::Value::Null,
            "missing scheduled tasks record",
        ),
        (
            "/scheduled_tasks/schema",
            serde_json::json!(3),
            "unsupported scheduled tasks schema",
        ),
        (
            "/scheduled_tasks/pending/0/operation",
            serde_json::Value::Null,
            "missing scheduled task operation",
        ),
        (
            "/scheduled_tasks/pending/0/due_tick",
            serde_json::json!(1),
            "invalid or duplicate saved scheduled task",
        ),
        (
            "/scheduled_tasks/pending/1/id",
            serde_json::json!(1),
            "invalid or duplicate saved scheduled task",
        ),
        (
            "/schema",
            serde_json::json!(7),
            "manifest schema 7 must not declare scheduled tasks record",
        ),
    ] {
        let mut corrupted = valid.clone();
        *corrupted.pointer_mut(field).unwrap() = value;
        fs::write(&path, serde_json::to_vec(&corrupted).unwrap()).unwrap();
        assert!(
            load_existing(&world)
                .err()
                .unwrap()
                .to_string()
                .contains(diagnostic)
        );
        assert!(
            migrate_checkpoint(&world, &destination)
                .unwrap_err()
                .to_string()
                .contains(diagnostic)
        );
        assert!(!destination.exists());
    }
}

#[test]
fn restored_operations_observe_same_tick_edits_and_player_occupancy() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("source");
    let destination = root.path().join("migrated");
    let mut server = Server::playable(42).unwrap();
    let empty = server.registry().block_id("rustcraft:empty").unwrap();
    let basalt = server
        .registry()
        .block_id("rustcraft_content:basalt")
        .unwrap();
    let loam = server
        .registry()
        .block_id("rustcraft_content:loam")
        .unwrap();
    let origin = ChunkPos::ORIGIN;
    let neighbor = ChunkPos { x: 1, z: 0 };
    let target = BlockPos::new(2, 0, 2);
    let changed = BlockPos::new(3, 0, 2);
    let place = server
        .schedule_operation(
            2,
            ScheduledOperation::ReplaceBlock {
                position: target,
                expected: empty,
                replacement: basalt,
            },
        )
        .unwrap();
    let stale = server
        .schedule_operation(
            2,
            ScheduledOperation::ReplaceBlock {
                position: changed,
                expected: empty,
                replacement: basalt,
            },
        )
        .unwrap();
    let deactivate = server
        .schedule_operation(
            2,
            ScheduledOperation::DeactivateChunk { position: neighbor },
        )
        .unwrap();
    let occupied = server
        .schedule_operation(2, ScheduledOperation::DeactivateChunk { position: origin })
        .unwrap();
    let canceled = server
        .schedule_operation(
            2,
            ScheduledOperation::ReplaceBlock {
                position: target,
                expected: basalt,
                replacement: empty,
            },
        )
        .unwrap();
    assert!(server.cancel_task(canceled));
    assert!(server.step(vec![]).due_tasks.is_empty());
    let checkpoint = server.checkpoint().unwrap();
    save(&world, None, &server).unwrap();
    migrate_checkpoint(&world, &destination).unwrap();

    let (_, mut resumed) = load_existing(&destination).unwrap();
    assert_eq!(resumed.checkpoint().unwrap(), checkpoint);
    let session = resumed.open_local_session(Capabilities::P1_HOST);
    let expected_revision = resumed.snapshot().chunks[&origin].revision();
    let report = resumed.step(vec![Request {
        session,
        operation: 1,
        mutation: Mutation::SetBlock {
            position: changed,
            state: loam,
            expected_revision,
        },
    }]);
    assert_eq!(report.acks[0].result, Ok(Effect::ChunkChanged(origin)));
    assert_eq!(report.due_tasks, [place, stale, deactivate, occupied]);
    let results: Vec<_> = report
        .task_events
        .into_iter()
        .map(|event| match event {
            TaskEvent::Due { task, result, .. } => (task, result),
            event => panic!("unexpected event after restoration: {event:?}"),
        })
        .collect();
    assert_eq!(
        results,
        [
            (place, Ok(Effect::ChunkChanged(origin))),
            (stale, Err(Rejection::UnexpectedBlockState { actual: loam })),
            (deactivate, Ok(Effect::ChunkChanged(neighbor))),
            (occupied, Err(Rejection::ChunkOccupied)),
        ]
    );
    let chunk = &report.snapshot.chunks[&origin];
    assert_eq!(chunk.block(target), Some(basalt));
    assert_eq!(chunk.block(changed), Some(loam));
    assert!(!report.snapshot.chunks.contains_key(&neighbor));
    assert!(resumed.step(vec![]).task_events.is_empty());
}

#[test]
fn schema_one_tasks_are_loaded_and_migrated_as_plain_events() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("source");
    let destination = root.path().join("migrated");
    let mut server = Server::playable(42).unwrap();
    let task = server.schedule_task(1).unwrap();
    save(&world, None, &server).unwrap();
    let path = world.join("checkpoints/1/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let record = &mut manifest["scheduled_tasks"];
    record["schema"] = 1.into();
    record["pending"][0]
        .as_object_mut()
        .unwrap()
        .remove("operation")
        .unwrap();
    fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();

    migrate_checkpoint(&world, &destination).unwrap();
    let upgraded: serde_json::Value =
        serde_json::from_slice(&fs::read(destination.join("checkpoints/1/manifest.json")).unwrap())
            .unwrap();
    assert_eq!(upgraded["scheduled_tasks"]["schema"], 2);
    for path in [&world, &destination] {
        let (_, mut resumed) = load_existing(path).unwrap();
        let report = resumed.step(vec![]);
        assert_eq!(report.due_tasks, [task]);
        assert_eq!(
            report.task_events,
            [TaskEvent::Due {
                task,
                operation: ScheduledOperation::Event,
                result: Ok(Effect::Unchanged)
            }]
        );
    }
}

#[test]
fn a_full_operation_queue_can_be_saved_and_loaded() {
    let world = tempfile::tempdir().unwrap();
    let mut server = Server::playable(42).unwrap();
    let expected = server
        .registry()
        .block_id("rustcraft_content:lichenstone")
        .unwrap();
    let replacement = server
        .registry()
        .block_id("rustcraft_content:basalt")
        .unwrap();
    let operation = ScheduledOperation::ReplaceBlock {
        position: BlockPos::new(i64::MIN, 255, i64::MIN),
        expected,
        replacement,
    };
    for _ in 0..4096 {
        server.schedule_operation(u64::MAX - 1, operation).unwrap();
    }
    assert_eq!(
        server.schedule_operation(1, operation),
        Err("scheduled task queue is full")
    );
    save(world.path(), None, &server).unwrap();
    let (_, loaded) = load_existing(world.path()).unwrap();
    let checkpoint = loaded.checkpoint().unwrap();
    assert_eq!(checkpoint.scheduled_tasks.pending.len(), 4096);
    assert_eq!(checkpoint, server.checkpoint().unwrap());
}
