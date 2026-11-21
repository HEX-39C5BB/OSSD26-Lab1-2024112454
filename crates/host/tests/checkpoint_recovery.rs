use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use rustcraft_authority::Server;
use rustcraft_persistence::{load_existing, save};

fn run(flag: &str, world: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rustcraft-server"));
    command.arg(flag).arg(world);
    command
}

fn restore(world: &Path, generation: &str, destination: &Path) -> Output {
    run("--restore-generation", world)
        .arg(generation)
        .arg(destination)
        .output()
        .unwrap()
}

#[test]
fn verifies_only_the_selected_generation_without_a_valid_current_or_a_write_lock() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path();
    let mut server = Server::playable(42).unwrap();
    assert_eq!(save(world, None, &server).unwrap(), 1);
    server.step(vec![]);
    assert_eq!(save(world, Some(1), &server).unwrap(), 2);
    fs::write(world.join("CURRENT"), b"damaged pointer").unwrap();
    fs::remove_file(world.join(".lock")).unwrap();
    let manifest = world.join("checkpoints/1/manifest.json");
    let original_manifest = fs::read(&manifest).unwrap();
    let region = world.join("checkpoints/2/r.0.0.rcr");
    let mut damaged = fs::read(&region).unwrap();
    *damaged.last_mut().unwrap() ^= 1;
    fs::write(&region, &damaged).unwrap();

    let output = run("--verify-generation", world).arg("1").output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.starts_with("event=checkpoint_verified generation=1 tick=0 seed=42 active_chunks=9")
    );
    assert!(stdout.contains("seed_derivation=rustcraft:seed_derivation@1"));
    assert!(output.stderr.is_empty());

    for (generation, diagnostic) in [
        ("2", "checkpoint 2 region r.0.0.rcr: CRC32C mismatch"),
        ("3", "checkpoint generation 3"),
        ("0", "GENERATION must be a positive integer"),
    ] {
        let output = run("--verify-generation", world)
            .arg(generation)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains(diagnostic)
        );
    }

    assert!(!world.join(".lock").exists());
    assert_eq!(fs::read(world.join("CURRENT")).unwrap(), b"damaged pointer");
    assert_eq!(fs::read(manifest).unwrap(), original_manifest);
    assert_eq!(fs::read(region).unwrap(), damaged);
    assert_eq!(fs::read_dir(world.join("checkpoints")).unwrap().count(), 2);
}

#[test]
fn lists_numbered_candidates_even_when_current_is_unreadable() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path();
    let checkpoints = world.join("checkpoints");
    let mut server = Server::playable(42).unwrap();
    assert_eq!(save(world, None, &server).unwrap(), 1);
    server.step(vec![]);
    assert_eq!(save(world, Some(1), &server).unwrap(), 2);
    server.step(vec![]);
    assert_eq!(save(world, Some(2), &server).unwrap(), 3);
    fs::write(world.join("CURRENT"), b"2\n").unwrap();
    fs::write(checkpoints.join("1/manifest.json"), b"damaged").unwrap();
    fs::write(checkpoints.join("2/r.0.0.rcr"), b"damaged").unwrap();
    fs::create_dir(checkpoints.join(".pending-interrupted")).unwrap();
    fs::create_dir(checkpoints.join("03")).unwrap();
    fs::create_dir(checkpoints.join("0")).unwrap();
    fs::write(checkpoints.join("4"), b"not a directory").unwrap();

    let output = run("--list-checkpoints", world).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0], "event=checkpoint_candidates current=2 count=3");
    assert!(lines[1].starts_with(
        "event=checkpoint_candidate generation=1 tick=unknown current=false status=manifest_error"
    ));
    assert_eq!(
        lines[2],
        "event=checkpoint_candidate generation=2 tick=1 current=true status=manifest_ok"
    );
    assert_eq!(
        lines[3],
        "event=checkpoint_candidate generation=3 tick=2 current=false status=manifest_ok"
    );
    assert_eq!(fs::read(world.join("CURRENT")).unwrap(), b"2\n");

    let output = run("--verify-world", world).output().unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("checkpoint 2 region r.0.0.rcr")
    );
    fs::write(world.join("CURRENT"), b"damaged").unwrap();
    let output = run("--list-checkpoints", world).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("event=checkpoint_pointer_unreadable")
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("event=checkpoint_candidates current=unreadable count=3\n"));
    assert!(
        stdout
            .lines()
            .skip(1)
            .all(|line| line.contains("current=unknown"))
    );
    assert_eq!(fs::read(world.join("CURRENT")).unwrap(), b"damaged");
}

#[test]
fn restores_only_the_selected_valid_generation_without_updating_source() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let recovered = root.path().join("recovered");
    let failed = root.path().join("failed");
    let mut server = Server::playable(42).unwrap();
    assert_eq!(save(&world, None, &server).unwrap(), 1);
    server.step(vec![]);
    let expected = server.checkpoint().unwrap();
    assert_eq!(save(&world, Some(1), &server).unwrap(), 2);
    server.step(vec![]);
    assert_eq!(save(&world, Some(2), &server).unwrap(), 3);
    let manifest = fs::read(world.join("checkpoints/2/manifest.json")).unwrap();
    let region = world.join("checkpoints/3/r.0.0.rcr");
    fs::write(&region, b"damaged").unwrap();
    fs::remove_file(world.join("CURRENT")).unwrap();

    for (generation, diagnostic) in [
        ("0", "positive integer"),
        ("4", "checkpoint generation 4"),
        ("3", "checkpoint 3 region r.0.0.rcr"),
    ] {
        let output = restore(&world, generation, &failed);
        assert!(!output.status.success(), "{output:?}");
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains(diagnostic)
        );
        assert!(!failed.exists());
    }

    let output = restore(&world, "2", &recovered);
    assert!(output.status.success(), "{output:?}");
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("event=checkpoint_restored generation=2")
    );
    assert_eq!(fs::read(recovered.join("CURRENT")).unwrap(), b"2\n");
    assert_eq!(
        fs::read(recovered.join("checkpoints/2/manifest.json")).unwrap(),
        manifest
    );
    assert_eq!(
        fs::read_dir(recovered.join("checkpoints")).unwrap().count(),
        1
    );
    let (generation, mut loaded) = load_existing(&recovered).unwrap();
    assert_eq!(generation, 2);
    assert_eq!(loaded.checkpoint().unwrap(), expected);
    assert!(!restore(&world, "2", &recovered).status.success());
    loaded.step(vec![]);
    assert_eq!(save(&recovered, Some(generation), &loaded).unwrap(), 3);
    assert!(!world.join("CURRENT").exists());
    assert_eq!(fs::read(region).unwrap(), b"damaged");
}
