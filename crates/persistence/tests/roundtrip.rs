use std::fs;

use rustcraft_authority::{
    BlockAction, Capabilities, Effect, InventoryAction, Mutation, PlayerInput, Rejection, Request,
    RulePreset, SeedScope, Server, SessionId, ViewDistance,
};
use rustcraft_persistence::{copy_checkpoint, load, migrate_checkpoint, save};
use rustcraft_world::{BlockPos, ChunkPos, VerticalRange};

fn remove_post_schema_six_fields(manifest: &mut serde_json::Value) {
    manifest
        .as_object_mut()
        .unwrap()
        .remove("scheduled_tasks")
        .unwrap();
    manifest
        .as_object_mut()
        .unwrap()
        .remove("random_streams")
        .unwrap();
    manifest["rules"]
        .as_object_mut()
        .unwrap()
        .remove("random_stream")
        .unwrap();
}

fn request(
    server: &mut Server,
    session: SessionId,
    operation: u64,
    mutation: Mutation,
) -> Result<Effect, Rejection> {
    server
        .step(vec![Request {
            session,
            operation,
            mutation,
        }])
        .acks
        .remove(0)
        .result
}

#[test]
fn saves_and_restores_blocks_player_inventory_and_height() {
    let root = tempfile::tempdir().unwrap();
    let height = VerticalRange {
        min: -80,
        max_exclusive: 272,
    };
    let mut server = Server::playable_with_range(42, height).unwrap();
    let session = server.open_player_session().unwrap();
    for _ in 0..16 {
        server.step(vec![]);
    }
    request(
        &mut server,
        session,
        1,
        Mutation::ControlPlayer(PlayerInput {
            forward: 1,
            pitch: -0.65,
            ..PlayerInput::default()
        }),
    )
    .unwrap();
    request(
        &mut server,
        session,
        2,
        Mutation::ControlPlayer(PlayerInput {
            pitch: -0.65,
            ..PlayerInput::default()
        }),
    )
    .unwrap();

    let before = server.snapshot();
    let initial = before.inventory.unwrap();
    let target = before.player.unwrap().target.unwrap();
    let placed = target.placement.unwrap().position;
    assert_eq!(
        request(
            &mut server,
            session,
            3,
            Mutation::UseBlock {
                action: BlockAction::Place,
                target
            }
        ),
        Ok(Effect::ChunkChanged(placed.chunk()))
    );
    let inventory = server.snapshot().inventory.unwrap();
    assert_eq!(
        inventory.slots()[0].unwrap().count,
        initial.slots()[0].unwrap().count - 1
    );
    assert_eq!(
        request(
            &mut server,
            session,
            4,
            Mutation::EditInventory {
                action: InventoryAction::Transfer {
                    from: 0,
                    to: 4,
                    count: 3
                },
                expected_revision: inventory.revision(),
            }
        ),
        Ok(Effect::InventoryChanged)
    );
    let moved = server.snapshot().inventory.unwrap();
    assert_eq!(
        request(
            &mut server,
            session,
            5,
            Mutation::EditInventory {
                action: InventoryAction::Select { slot: 4 },
                expected_revision: moved.revision(),
            }
        ),
        Ok(Effect::InventoryChanged)
    );

    server.close_session(session);
    let checkpoint = server.checkpoint().unwrap();
    assert_eq!(save(root.path(), None, &server).unwrap(), 1);
    let (generation, mut loaded) = load(root.path()).unwrap().unwrap();
    assert_eq!(generation, 1);
    assert_eq!(checkpoint, loaded.checkpoint().unwrap());
    assert_eq!(loaded.snapshot().height, Some(height));

    let session = loaded.open_player_session().unwrap();
    let target = loaded.snapshot().player.unwrap().target.unwrap();
    assert_eq!(target.position, placed);
    assert_eq!(
        request(
            &mut loaded,
            session,
            1,
            Mutation::UseBlock {
                action: BlockAction::Break,
                target
            }
        ),
        Ok(Effect::ChunkChanged(placed.chunk()))
    );
    assert_eq!(
        loaded.snapshot().inventory.unwrap().slots()[0]
            .unwrap()
            .count,
        moved.slots()[0].unwrap().count + 1
    );
    assert_eq!(save(root.path(), Some(generation), &loaded).unwrap(), 2);
}

#[test]
fn rejects_stale_saves_and_ignores_unpublished_generations() {
    let root = tempfile::tempdir().unwrap();
    let mut server = Server::playable(42).unwrap();
    assert_eq!(save(root.path(), None, &server).unwrap(), 1);
    fs::create_dir(root.path().join("checkpoints").join("2")).unwrap();
    assert_eq!(load(root.path()).unwrap().unwrap().0, 1);
    assert!(save(root.path(), None, &server).is_err());
    server.step(vec![]);
    assert_eq!(save(root.path(), Some(1), &server).unwrap(), 3);
    assert_eq!(load(root.path()).unwrap().unwrap().1.snapshot().tick, 1);
    assert!(
        root.path()
            .join("checkpoints")
            .join("1/manifest.json")
            .is_file()
    );
    assert!(save(root.path(), Some(1), &server).is_err());
}

#[test]
fn backs_up_the_published_generation_and_restores_an_independent_world() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let backup = root.path().join("backup");
    let restored = root.path().join("restored");
    let mut server = Server::playable(42).unwrap();
    save(&world, None, &server).unwrap();
    server.step(vec![]);
    let checkpoint = server.checkpoint().unwrap();
    save(&world, Some(1), &server).unwrap();
    fs::create_dir(world.join("checkpoints/3")).unwrap();

    assert_eq!(copy_checkpoint(&world, &backup).unwrap(), 2);
    assert_eq!(fs::read_dir(backup.join("checkpoints")).unwrap().count(), 1);
    assert_eq!(
        load(&backup).unwrap().unwrap().1.checkpoint().unwrap(),
        checkpoint
    );

    server.step(vec![]);
    assert_eq!(save(&world, Some(2), &server).unwrap(), 4);
    assert_eq!(copy_checkpoint(&backup, &restored).unwrap(), 2);
    let (generation, mut restored_server) = load(&restored).unwrap().unwrap();
    assert_eq!(generation, 2);
    assert_eq!(restored_server.checkpoint().unwrap(), checkpoint);
    restored_server.step(vec![]);
    assert_eq!(
        save(&restored, Some(generation), &restored_server).unwrap(),
        3
    );
    assert_eq!(load(&backup).unwrap().unwrap().0, 2);
    assert_eq!(load(&world).unwrap().unwrap().0, 4);
}

#[test]
fn refuses_existing_destinations_and_leaves_no_copy_of_a_corrupt_checkpoint() {
    use std::io::ErrorKind;

    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let existing = root.path().join("existing");
    let destination = root.path().join("destination");
    save(&world, None, &Server::playable(42).unwrap()).unwrap();
    fs::create_dir(&existing).unwrap();
    assert_eq!(
        copy_checkpoint(&world, &existing).unwrap_err().kind(),
        ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read_dir(&existing).unwrap().count(), 0);
    assert_eq!(
        migrate_checkpoint(&world, &existing).unwrap_err().kind(),
        ErrorKind::AlreadyExists
    );
    assert_eq!(
        copy_checkpoint(&world, &world.join("nested"))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        migrate_checkpoint(&world, &world.join("nested"))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidInput
    );
    let corrupted = world.join("checkpoints/1/player.cbor");
    fs::write(corrupted, b"damaged").unwrap();
    let error = copy_checkpoint(&world, &destination).unwrap_err();
    assert!(error.to_string().contains("CRC32C mismatch"));
    assert!(!destination.exists());
    let error = migrate_checkpoint(&world, &destination).unwrap_err();
    assert!(error.to_string().contains("CRC32C mismatch"));
    assert!(!destination.exists());
    assert_eq!(fs::read_to_string(world.join("CURRENT")).unwrap(), "1\n");
}

#[test]
fn rejects_corrupted_records_and_unknown_manifest_versions() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::playable(42).unwrap();
    save(root.path(), None, &server).unwrap();
    let checkpoint = root.path().join("checkpoints/1");
    let region = checkpoint.join("r.0.0.rcr");
    let mut bytes = fs::read(&region).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(region, bytes).unwrap();
    let error = load(root.path()).err().unwrap();
    assert!(error.to_string().contains("CRC32C mismatch"));

    let path = checkpoint.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["schema"] = serde_json::json!(9);
    fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let error = load(root.path()).err().unwrap();
    assert!(
        error
            .to_string()
            .contains("unsupported manifest schema version 9")
    );
    assert_eq!(
        fs::read_to_string(root.path().join("CURRENT")).unwrap(),
        "1\n"
    );
}

#[test]
fn legacy_world_generates_the_same_terrain_after_reload() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let backup = root.path().join("backup");
    let mut uninterrupted = Server::playable(42).unwrap();
    let position = ChunkPos { x: 0, z: 2 };
    assert!(!uninterrupted.snapshot().chunks.contains_key(&position));
    save(&world, None, &uninterrupted).unwrap();

    let manifest_path = world.join("checkpoints/1/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["schema"] = serde_json::json!(1);
    remove_post_schema_six_fields(&mut manifest);
    manifest.as_object_mut().unwrap().remove("rules").unwrap();
    let legacy = serde_json::to_vec(&manifest).unwrap();
    fs::write(&manifest_path, &legacy).unwrap();

    assert_eq!(copy_checkpoint(&world, &backup).unwrap(), 1);
    assert_eq!(
        fs::read(backup.join("checkpoints/1/manifest.json")).unwrap(),
        legacy
    );
    let (generation, mut resumed) = load(&world).unwrap().unwrap();
    assert_eq!(resumed.rules(), RulePreset::NativeV1);
    let scope = SeedScope::Chunk {
        dimension: "rustcraft:overworld",
        position,
    };
    assert_eq!(
        resumed
            .world_seeds()
            .for_system(scope, "rustcraft:features"),
        uninterrupted
            .world_seeds()
            .for_system(scope, "rustcraft:features")
    );
    assert_eq!(load(&backup).unwrap().unwrap().1.rules(), resumed.rules());
    assert_eq!(fs::read(&manifest_path).unwrap(), legacy);

    for server in [&mut uninterrupted, &mut resumed] {
        server.set_local_view_distance(ViewDistance::from_radius(1).unwrap());
        let session = server.open_player_session().unwrap();
        request(
            server,
            session,
            1,
            Mutation::ControlPlayer(PlayerInput {
                forward: 1,
                ..PlayerInput::default()
            }),
        )
        .unwrap();
        for _ in 0..140 {
            server.step(vec![]);
        }
        assert_eq!(
            server.snapshot().player.unwrap().block_position().chunk(),
            position
        );
    }
    let resumed_checkpoint = resumed.checkpoint().unwrap();
    assert!(resumed_checkpoint == uninterrupted.checkpoint().unwrap());

    // Each bit is the original seed-42 basalt sample at y=-10 for that column.
    let basalt = resumed
        .registry()
        .block_id("rustcraft_content:basalt")
        .unwrap();
    let snapshot = resumed.snapshot();
    let chunk = &snapshot.chunks[&position];
    let basalt_rows: Vec<u16> = (32..48)
        .map(|z| {
            (0..16).fold(0, |bits, x| {
                let sampled = chunk.block(BlockPos::new(x, -10, z)) == Some(basalt);
                bits | (u16::from(sampled) << x)
            })
        })
        .collect();
    assert_eq!(
        basalt_rows,
        [
            4160, 4104, 20480, 8321, 4096, 41092, 10588, 35872, 2, 1024, 16, 2056, 8, 16384, 2, 64
        ]
    );

    assert_eq!(save(&world, Some(generation), &resumed).unwrap(), 2);
    assert_eq!(fs::read(&manifest_path).unwrap(), legacy);
    let new_manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(world.join("checkpoints/2/manifest.json")).unwrap())
            .unwrap();
    assert_eq!(new_manifest["schema"], 8);
    assert_eq!(
        new_manifest["rules"],
        serde_json::json!({
            "preset": { "id": "rustcraft:native", "version": 1 },
            "world_generator": { "id": "rustcraft:flat", "version": 1 },
            "tick_schedule": { "id": "rustcraft:tick_schedule", "version": 1 },
            "player_movement": { "id": "rustcraft:player_movement", "version": 1 },
            "block_inventory": { "id": "rustcraft:block_inventory", "version": 1 },
            "seed_derivation": { "id": "rustcraft:seed_derivation", "version": 1 },
            "random_stream": { "id": "rustcraft:random_stream", "version": 1 }
        })
    );
    assert_eq!(
        load(&world).unwrap().unwrap().1.checkpoint().unwrap(),
        resumed_checkpoint
    );
}

#[test]
fn loads_schema_five_with_the_original_seed_derivation() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::playable(42).unwrap();
    save(root.path(), None, &server).unwrap();
    let path = root.path().join("checkpoints/1/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["schema"] = serde_json::json!(5);
    remove_post_schema_six_fields(&mut manifest);
    manifest["rules"]
        .as_object_mut()
        .unwrap()
        .remove("seed_derivation")
        .unwrap();
    fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();

    let (_, restored) = load(root.path()).unwrap().unwrap();
    assert_eq!(restored.checkpoint().unwrap(), server.checkpoint().unwrap());
    assert_eq!(
        restored.world_seeds().for_scope(SeedScope::World),
        0x54ae_7685_c80e_0532
    );
}

#[test]
fn loads_schema_two_with_its_original_tick_schedule() {
    let root = tempfile::tempdir().unwrap();
    let mut server = Server::playable(42).unwrap();
    server.step(vec![]);
    assert_eq!(save(root.path(), None, &server).unwrap(), 1);
    let path = root.path().join("checkpoints/1/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["schema"] = serde_json::json!(2);
    remove_post_schema_six_fields(&mut manifest);
    let rules = manifest["rules"].as_object_mut().unwrap();
    rules.remove("tick_schedule").unwrap();
    rules.remove("player_movement").unwrap();
    rules.remove("block_inventory").unwrap();
    rules.remove("seed_derivation").unwrap();
    let legacy = serde_json::to_vec(&manifest).unwrap();
    fs::write(&path, &legacy).unwrap();

    let (generation, mut resumed) = load(root.path()).unwrap().unwrap();
    assert_eq!(resumed.rules(), RulePreset::NativeV1);
    resumed.step(vec![]);
    server.step(vec![]);
    assert!(resumed.checkpoint().unwrap() == server.checkpoint().unwrap());
    assert_eq!(save(root.path(), Some(generation), &resumed).unwrap(), 2);
    assert_eq!(fs::read(&path).unwrap(), legacy);
}

#[test]
fn rejects_missing_or_unsupported_rule_locks_without_publishing() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let backup = root.path().join("backup");
    let server = Server::playable(42).unwrap();
    save(&world, None, &server).unwrap();
    let path = world.join("checkpoints/1/manifest.json");
    let valid: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();

    for (pointer, value, expected) in [
        (
            "/rules",
            serde_json::Value::Null,
            "missing rules in manifest schema 8",
        ),
        (
            "/rules/preset/id",
            serde_json::json!("rustcraft:strict"),
            "unsupported rule preset",
        ),
        (
            "/rules/preset/version",
            serde_json::json!(2),
            "unsupported rule preset",
        ),
        (
            "/rules/world_generator/id",
            serde_json::json!("rustcraft:other"),
            "unsupported world generator",
        ),
        (
            "/rules/world_generator/version",
            serde_json::json!(2),
            "unsupported world generator",
        ),
        (
            "/rules/tick_schedule",
            serde_json::Value::Null,
            "missing tick schedule in manifest rules",
        ),
        (
            "/rules/tick_schedule/id",
            serde_json::json!("rustcraft:other"),
            "unsupported tick schedule",
        ),
        (
            "/rules/tick_schedule/version",
            serde_json::json!(2),
            "unsupported tick schedule",
        ),
        (
            "/schema",
            serde_json::json!(2),
            "manifest schema 2 must not declare tick schedule",
        ),
        (
            "/rules/player_movement",
            serde_json::Value::Null,
            "missing player movement in manifest rules",
        ),
        (
            "/rules/player_movement/id",
            serde_json::json!("rustcraft:other"),
            "unsupported player movement",
        ),
        (
            "/rules/player_movement/version",
            serde_json::json!(2),
            "unsupported player movement",
        ),
        (
            "/schema",
            serde_json::json!(3),
            "manifest schema 3 must not declare player movement",
        ),
        (
            "/rules/block_inventory",
            serde_json::Value::Null,
            "missing block and inventory rules in manifest rules",
        ),
        (
            "/rules/block_inventory/id",
            serde_json::json!("rustcraft:other"),
            "unsupported block and inventory rules",
        ),
        (
            "/rules/block_inventory/version",
            serde_json::json!(2),
            "unsupported block and inventory rules",
        ),
        (
            "/schema",
            serde_json::json!(4),
            "manifest schema 4 must not declare block and inventory rules",
        ),
        (
            "/rules/seed_derivation",
            serde_json::Value::Null,
            "missing seed derivation in manifest rules",
        ),
        (
            "/rules/seed_derivation/id",
            serde_json::json!("rustcraft:other"),
            "unsupported seed derivation",
        ),
        (
            "/rules/seed_derivation/version",
            serde_json::json!(2),
            "unsupported seed derivation",
        ),
        (
            "/schema",
            serde_json::json!(5),
            "manifest schema 5 must not declare seed derivation",
        ),
        (
            "/rules/random_stream",
            serde_json::Value::Null,
            "missing random stream in manifest rules",
        ),
        (
            "/rules/random_stream/version",
            serde_json::json!(2),
            "unsupported random stream",
        ),
        (
            "/random_streams",
            serde_json::Value::Null,
            "missing random streams record",
        ),
        (
            "/random_streams/schema",
            serde_json::json!(2),
            "unsupported random streams schema version 2",
        ),
        (
            "/schema",
            serde_json::json!(6),
            "manifest schema 6 must not declare random stream",
        ),
        (
            "/schema",
            serde_json::json!(1),
            "legacy manifest must not declare rules",
        ),
    ] {
        let mut manifest = valid.clone();
        *manifest.pointer_mut(pointer).unwrap() = value;
        fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(
            load(&world).err().unwrap().to_string().contains(expected),
            "{pointer}"
        );
        assert!(
            migrate_checkpoint(&world, &backup)
                .unwrap_err()
                .to_string()
                .contains(expected),
            "{pointer}"
        );
        assert!(
            copy_checkpoint(&world, &backup)
                .unwrap_err()
                .to_string()
                .contains(expected),
            "{pointer}"
        );
        assert!(
            save(&world, Some(1), &server)
                .unwrap_err()
                .to_string()
                .contains(expected),
            "{pointer}"
        );
    }
    assert!(!backup.exists());
    assert_eq!(fs::read_to_string(world.join("CURRENT")).unwrap(), "1\n");
    assert_eq!(fs::read_dir(world.join("checkpoints")).unwrap().count(), 1);
}

#[test]
fn legacy_manifest_preserves_flight_cancellation_and_landing() {
    let root = tempfile::tempdir().unwrap();
    let mut original = Server::playable(42).unwrap();
    for _ in 0..16 {
        original.step(vec![]);
    }
    let session = original.open_player_session().unwrap();
    request(
        &mut original,
        session,
        1,
        Mutation::ControlPlayer(PlayerInput {
            jump: true,
            jump_presses: 2,
            ..PlayerInput::default()
        }),
    )
    .unwrap();
    for _ in 0..3 {
        original.step(vec![]);
    }
    original.close_session(session);
    assert!(original.snapshot().player.unwrap().flying);
    assert_eq!(save(root.path(), None, &original).unwrap(), 1);
    let path = root.path().join("checkpoints/1/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["schema"] = serde_json::json!(3);
    remove_post_schema_six_fields(&mut manifest);
    let rules = manifest["rules"].as_object_mut().unwrap();
    rules.remove("player_movement").unwrap();
    rules.remove("block_inventory").unwrap();
    rules.remove("seed_derivation").unwrap();
    let legacy = serde_json::to_vec(&manifest).unwrap();
    fs::write(&path, &legacy).unwrap();

    let (generation, mut resumed) = load(root.path()).unwrap().unwrap();
    assert_eq!(resumed.snapshot().player, original.snapshot().player);
    for server in [&mut original, &mut resumed] {
        let session = server.open_player_session().unwrap();
        request(
            server,
            session,
            1,
            Mutation::ControlPlayer(PlayerInput {
                jump_presses: 2,
                ..PlayerInput::default()
            }),
        )
        .unwrap();
        let cancelled = server.snapshot().player.unwrap();
        assert!(!cancelled.flying && !cancelled.grounded);
        assert!((cancelled.position[1] - 1.14).abs() < 1e-9);
    }
    for _ in 0..20 {
        assert_eq!(
            original.step(vec![]).snapshot.player,
            resumed.step(vec![]).snapshot.player
        );
    }
    assert!(resumed.snapshot().player.unwrap().grounded);
    assert_eq!(save(root.path(), Some(generation), &resumed).unwrap(), 2);
    assert_eq!(fs::read(path).unwrap(), legacy);
}

#[test]
fn schema_four_preserves_inventory_and_block_transactions_after_reload() {
    let root = tempfile::tempdir().unwrap();
    let mut original = Server::playable(42).unwrap();
    for _ in 0..16 {
        original.step(vec![]);
    }
    let session = original.open_player_session().unwrap();
    request(
        &mut original,
        session,
        1,
        Mutation::ControlPlayer(PlayerInput {
            pitch: -0.65,
            ..PlayerInput::default()
        }),
    )
    .unwrap();
    original.close_session(session);
    assert_eq!(save(root.path(), None, &original).unwrap(), 1);
    let path = root.path().join("checkpoints/1/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["schema"] = serde_json::json!(4);
    remove_post_schema_six_fields(&mut manifest);
    let rules = manifest["rules"].as_object_mut().unwrap();
    rules.remove("block_inventory").unwrap();
    rules.remove("seed_derivation").unwrap();
    let legacy = serde_json::to_vec(&manifest).unwrap();
    fs::write(&path, &legacy).unwrap();

    let (generation, mut resumed) = load(root.path()).unwrap().unwrap();
    for server in [&mut original, &mut resumed] {
        let session = server.open_player_session().unwrap();
        let inventory = server.snapshot().inventory.unwrap();
        assert_eq!(
            request(
                server,
                session,
                1,
                Mutation::EditInventory {
                    action: InventoryAction::Transfer {
                        from: 1,
                        to: 4,
                        count: 2
                    },
                    expected_revision: inventory.revision(),
                }
            ),
            Ok(Effect::InventoryChanged)
        );
        let inventory = server.snapshot().inventory.unwrap();
        assert_eq!(
            request(
                server,
                session,
                2,
                Mutation::EditInventory {
                    action: InventoryAction::Select { slot: 1 },
                    expected_revision: inventory.revision(),
                }
            ),
            Ok(Effect::InventoryChanged)
        );
        let before = server.snapshot();
        let target = before.player.unwrap().target.unwrap();
        let position = target.placement.unwrap().position;
        let place = Mutation::UseBlock {
            action: BlockAction::Place,
            target,
        };
        let chunk = position.chunk();
        assert_eq!(
            request(server, session, 3, place.clone()),
            Ok(Effect::ChunkChanged(chunk))
        );
        let placed = server.snapshot();
        assert_eq!(
            placed.inventory.as_ref().unwrap().slots()[1].unwrap().count,
            5
        );
        assert_eq!(
            request(server, session, 4, place),
            Err(Rejection::StaleBlockTarget)
        );
        let rejected = server.snapshot();
        assert_eq!(rejected.inventory, placed.inventory);
        assert_eq!(
            rejected.chunks[&chunk].revision(),
            placed.chunks[&chunk].revision()
        );

        let target = placed.player.unwrap().target.unwrap();
        assert_eq!(target.position, position);
        assert_eq!(
            request(
                server,
                session,
                5,
                Mutation::UseBlock {
                    action: BlockAction::Break,
                    target
                }
            ),
            Ok(Effect::ChunkChanged(chunk))
        );
        let cleared = server.snapshot();
        assert_eq!(
            cleared.chunks[&chunk].block(position),
            before.chunks[&chunk].block(position)
        );
        assert_eq!(
            cleared.inventory.unwrap().slots(),
            before.inventory.unwrap().slots()
        );
    }
    assert!(original.checkpoint().unwrap() == resumed.checkpoint().unwrap());
    assert_eq!(save(root.path(), Some(generation), &resumed).unwrap(), 2);
    assert_eq!(fs::read(path).unwrap(), legacy);
}

#[test]
fn resumes_saved_flight_without_resuming_held_keys() {
    let root = tempfile::tempdir().unwrap();
    let mut server = Server::playable(42).unwrap();
    let session = server.open_player_session().unwrap();
    request(
        &mut server,
        session,
        1,
        Mutation::ControlPlayer(PlayerInput {
            jump: true,
            jump_presses: 2,
            ..PlayerInput::default()
        }),
    )
    .unwrap();
    for _ in 0..4 {
        server.step(vec![]);
    }
    let in_flight = server.snapshot().player.unwrap();
    assert!(in_flight.flying);
    save(root.path(), None, &server).unwrap();

    let (_, mut resumed) = load(root.path()).unwrap().unwrap();
    assert_eq!(resumed.step(vec![]).snapshot.player.unwrap(), in_flight);
    let session = resumed.open_player_session().unwrap();
    request(
        &mut resumed,
        session,
        1,
        Mutation::ControlPlayer(PlayerInput {
            descend: true,
            ..PlayerInput::default()
        }),
    )
    .unwrap();
    for _ in 0..20 {
        resumed.step(vec![]);
    }
    let landed = resumed.snapshot().player.unwrap();
    assert!(landed.grounded && !landed.flying);
}

#[test]
fn rejects_edits_outside_the_default_vertical_range() {
    let mut server = Server::playable(42).unwrap();
    let admin = server.open_local_session(Capabilities::P1_HOST);
    let state = server
        .registry()
        .block_id("rustcraft_content:basalt")
        .unwrap();
    let snapshot = server.snapshot();
    assert_eq!(
        snapshot.height,
        Some(VerticalRange {
            min: -64,
            max_exclusive: 256
        })
    );
    let expected_revision = snapshot.chunks[&ChunkPos::ORIGIN].revision();
    for (operation, y) in [(1, -65), (2, 256)] {
        assert_eq!(
            request(
                &mut server,
                admin,
                operation,
                Mutation::SetBlock {
                    position: BlockPos::new(0, y, 0),
                    state,
                    expected_revision,
                }
            ),
            Err(Rejection::OutOfWorld)
        );
    }
    assert_eq!(
        server.snapshot().chunks[&ChunkPos::ORIGIN].revision(),
        expected_revision
    );
}
