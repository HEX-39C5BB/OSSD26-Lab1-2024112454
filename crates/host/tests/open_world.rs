use std::fs;
use std::process::Command;

use rustcraft_authority::Server;
use rustcraft_persistence::{copy_checkpoint, load_existing, save};

const SERVER: &str = env!("CARGO_BIN_EXE_rustcraft-server");
const WORLD_HOSTS: &[&str] = &[
    SERVER,
    #[cfg(all(target_os = "linux", feature = "client"))]
    env!("CARGO_BIN_EXE_rustcraft-client"),
];

#[test]
fn strict_open_refuses_missing_and_damaged_checkpoints() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing");
    let damaged = root.path().join("damaged");
    save(&damaged, None, &Server::playable(42).unwrap()).unwrap();
    fs::write(damaged.join("checkpoints/1/player.cbor"), b"damaged").unwrap();

    for binary in WORLD_HOSTS {
        for (path, expected) in [(&missing, "missing"), (&damaged, "CRC32C mismatch")] {
            let output = Command::new(binary)
                .arg("--open-world")
                .arg(path)
                .output()
                .unwrap();
            let error = String::from_utf8(output.stderr).unwrap();
            assert!(!output.status.success(), "{binary} opened {path:?}");
            assert!(error.contains(expected), "{error}");
            assert!(!error.contains("event=world_initialized"), "{error}");
            if path == &missing {
                assert!(
                    error.contains(&format!("missing {}", missing.join("CURRENT").display())),
                    "{error}"
                );
                assert!(!missing.exists());
            }
        }
    }
    assert_eq!(fs::read_to_string(damaged.join("CURRENT")).unwrap(), "1\n");
}

#[test]
fn strict_server_open_continues_a_restored_checkpoint() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let restored = root.path().join("restored");
    let mut server = Server::playable(42).unwrap();
    server.step(vec![]);
    assert_eq!(save(&source, None, &server).unwrap(), 1);
    assert_eq!(copy_checkpoint(&source, &restored).unwrap(), 1);

    let output = Command::new(SERVER)
        .arg("--open-world")
        .arg(&restored)
        .args(["--ticks", "1"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("event=checkpoint_loaded generation=1 tick=1")
    );
    let (generation, resumed) = load_existing(&restored).unwrap();
    assert_eq!(generation, 2);
    assert_eq!(resumed.snapshot().seed, 42);
    assert_eq!(resumed.snapshot().tick, 2);
    assert_eq!(fs::read_to_string(source.join("CURRENT")).unwrap(), "1\n");
}

#[test]
fn rejects_combining_open_and_create_options() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    for binary in WORLD_HOSTS {
        let output = Command::new(binary)
            .arg("--world")
            .arg(&world)
            .arg("--open-world")
            .arg(&world)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("mutually exclusive")
        );
        assert!(!world.exists());
    }
}

#[cfg(all(target_os = "linux", feature = "client"))]
#[test]
fn strict_client_open_loads_before_initializing_the_viewer() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let missing_pack = root.path().join("missing.rpak");
    let mut server = Server::playable(42).unwrap();
    server.step(vec![]);
    save(&world, None, &server).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rustcraft-client"))
        .arg("--open-world")
        .arg(&world)
        .arg("--pack")
        .arg(missing_pack)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(
        error.contains("event=checkpoint_loaded generation=1 tick=1"),
        "{error}"
    );
    assert!(error.contains("event=client_failed"), "{error}");
    assert_eq!(fs::read_to_string(world.join("CURRENT")).unwrap(), "1\n");
}

#[test]
fn launches_unset_modes_as_creative_and_remembers_explicit_override() {
    use rustcraft_authority::GameMode;
    let dir = tempfile::tempdir().unwrap();
    let world = dir.path().join("flat");
    save(&world, None, &Server::playable(42).unwrap()).unwrap();
    let run = |mode: Option<&str>| {
        let mut command = Command::new(SERVER);
        command
            .arg("--open-world")
            .arg(&world)
            .args(["--ticks", "0"]);
        if let Some(mode) = mode {
            command.args(["--gamemode", mode]);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        load_existing(&world).unwrap().1.game_mode()
    };
    assert_eq!(run(None), GameMode::Creative);
    assert_eq!(run(Some("survival")), GameMode::Survival);
    assert_eq!(run(None), GameMode::Survival);
}
