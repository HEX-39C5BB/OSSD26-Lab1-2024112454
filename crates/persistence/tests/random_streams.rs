use std::fs;

use rustcraft_authority::{SeedScope, Server};
use rustcraft_persistence::{load_existing, migrate_checkpoint, save};
use rustcraft_world::ChunkPos;

const SYSTEM: &str = "rustcraft:features";

fn draw(server: &mut Server, scope: SeedScope<'_>) -> u64 {
    let mut stream = server.system_random(scope, SYSTEM).unwrap();
    let sample = stream.next_u64();
    stream.commit();
    sample
}

#[test]
fn independent_streams_resume_after_save_and_migration() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let migrated_world = root.path().join("migrated");
    let mut original = Server::playable(42).unwrap();
    let world_scope = SeedScope::World;
    let dimension = SeedScope::Dimension("rustcraft:overworld");
    let chunk = SeedScope::Chunk {
        dimension: "rustcraft:overworld",
        position: ChunkPos { x: -7, z: 10 },
    };
    assert_eq!(draw(&mut original, world_scope), 0x6df0_9cfd_4756_03a3);
    assert_ne!(draw(&mut original, dimension), draw(&mut original, chunk));
    assert_eq!(draw(&mut original, world_scope), 0xe938_004b_51be_1553);

    let checkpoint = original.checkpoint().unwrap();
    assert_eq!(checkpoint.random_streams.len(), 3);
    {
        let mut aborted = original.system_random(world_scope, SYSTEM).unwrap();
        assert_eq!(aborted.next_u64(), 0x3aae_01d0_352f_85b5);
    }
    assert_eq!(original.checkpoint().unwrap(), checkpoint);
    assert_eq!(save(&world, None, &original).unwrap(), 1);
    assert_eq!(migrate_checkpoint(&world, &migrated_world).unwrap(), 1);
    let (_, mut restored) = load_existing(&world).unwrap();
    let (_, mut migrated) = load_existing(&migrated_world).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), checkpoint);
    assert_eq!(migrated.checkpoint().unwrap(), checkpoint);

    assert_eq!(draw(&mut original, world_scope), 0x3aae_01d0_352f_85b5);
    assert_eq!(draw(&mut restored, world_scope), 0x3aae_01d0_352f_85b5);
    assert_eq!(draw(&mut migrated, world_scope), 0x3aae_01d0_352f_85b5);
    for scope in [chunk, world_scope, dimension] {
        let expected = draw(&mut original, scope);
        assert_eq!(draw(&mut restored, scope), expected);
        assert_eq!(draw(&mut migrated, scope), expected);
    }
    assert_eq!(
        restored.checkpoint().unwrap(),
        original.checkpoint().unwrap()
    );
}

#[test]
fn rejects_duplicate_saved_stream_identities() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let destination = root.path().join("migration");
    let mut server = Server::playable(42).unwrap();
    draw(&mut server, SeedScope::World);
    save(&world, None, &server).unwrap();
    let path = world.join("checkpoints/1/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let streams = manifest["random_streams"]["streams"]
        .as_array_mut()
        .unwrap();
    streams.push(streams[0].clone());
    fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();

    let error = load_existing(&world).err().unwrap();
    assert!(error.to_string().contains("duplicate saved random stream"));
    let error = migrate_checkpoint(&world, &destination).unwrap_err();
    assert!(error.to_string().contains("duplicate saved random stream"));
    assert!(!destination.exists());
}
