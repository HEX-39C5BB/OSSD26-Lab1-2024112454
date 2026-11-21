use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use rustcraft_authority::Server;
use rustcraft_persistence::save;

fn verify(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rustcraft-server"))
        .arg("--verify-world")
        .arg(path)
        .output()
        .unwrap()
}

#[test]
fn verifies_only_the_published_checkpoint_without_advancing_it() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let mut server = Server::playable(42).unwrap();
    assert_eq!(save(&world, None, &server).unwrap(), 1);
    server.step(vec![]);
    assert_eq!(save(&world, Some(1), &server).unwrap(), 2);
    fs::write(world.join("checkpoints/1/manifest.json"), b"damaged").unwrap();
    let current = world.join("checkpoints/2");
    let manifest = fs::read(current.join("manifest.json")).unwrap();

    let output = verify(&world);
    assert!(output.status.success(), "{:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("event=checkpoint_verified generation=2 tick=1 seed=42 active_chunks=9")
    );
    assert!(stdout.contains("tick_schedule=rustcraft:tick_schedule@1"));
    assert!(stdout.contains("player_movement=rustcraft:player_movement@1"));
    assert!(stdout.contains("block_inventory=rustcraft:block_inventory@1"));
    assert!(stdout.contains("seed_derivation=rustcraft:seed_derivation@1"));
    assert!(stdout.contains("random_stream=rustcraft:random_stream@1"));
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read(current.join("manifest.json")).unwrap(), manifest);

    fs::write(current.join("player.cbor"), b"damaged").unwrap();
    let corrupt = verify(&world);
    assert!(!corrupt.status.success());
    assert!(
        String::from_utf8(corrupt.stderr)
            .unwrap()
            .contains("CRC32C mismatch")
    );
    assert!(corrupt.stdout.is_empty());
    assert_eq!(fs::read_to_string(world.join("CURRENT")).unwrap(), "2\n");
    assert_eq!(fs::read_dir(world.join("checkpoints")).unwrap().count(), 2);
}

#[test]
fn missing_world_fails_without_creating_a_directory() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("missing");
    let output = verify(&world);
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("no published checkpoint")
    );
    assert!(!world.exists());
}
