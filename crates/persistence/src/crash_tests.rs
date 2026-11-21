use std::io::Read;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::{
    Server, fs, load_existing, load_generation, prepare_checkpoint, publish_current, save,
};

const WORLD: &str = "RUSTCRAFT_CRASH_TEST_WORLD";
const READY: &str = "RUSTCRAFT_CRASH_TEST_READY";
const BOUNDARY: &str = "RUSTCRAFT_CRASH_TEST_BOUNDARY";

#[test]
#[ignore = "child process for the checkpoint interruption tests"]
fn hold_checkpoint_writer_at_publication_boundary() {
    let Some(world) = std::env::var_os(WORLD) else {
        return;
    };
    let world = Path::new(&world);
    let (generation, mut server) = load_existing(world).unwrap();
    server.step(vec![]);
    let prepared = prepare_checkpoint(world, Some(generation), &server).unwrap();
    match std::env::var(BOUNDARY).unwrap().as_str() {
        "before_current" => {}
        "after_current" => publish_current(world, prepared.generation).unwrap(),
        _ => panic!("unexpected publication boundary"),
    }
    fs::write(std::env::var_os(READY).unwrap(), b"ready").unwrap();
    // Keep the save lease alive until the parent kills this process or closes the pipe.
    let _ = std::io::stdin().read_exact(&mut [0]);
    drop(prepared);
}

fn kill_checkpoint_writer(world: &Path, ready: &Path, boundary: &str) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "crash_tests::hold_checkpoint_writer_at_publication_boundary",
            "--ignored",
            "--nocapture",
        ])
        .env(WORLD, world)
        .env(READY, ready)
        .env(BOUNDARY, boundary)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.is_file() {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("checkpoint writer exited before {boundary}: {status}");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("timed out waiting for checkpoint writer at {boundary}");
        }
        thread::sleep(Duration::from_millis(10));
    }
    child.kill().unwrap();
    assert_eq!(child.wait().unwrap().signal(), Some(9), "{boundary}");
}

fn assert_recovery_after_killing_writer(boundary: &str, expected_generation: u64) {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let mut server = Server::playable(42).unwrap();
    let previous = server.checkpoint().unwrap();
    assert_eq!(save(&world, None, &server).unwrap(), 1);
    server.step(vec![]);
    let replacement = server.checkpoint().unwrap();

    kill_checkpoint_writer(&world, &root.path().join("writer-ready"), boundary);

    assert_eq!(
        fs::read_to_string(world.join("CURRENT")).unwrap(),
        format!("{expected_generation}\n")
    );
    let (generation, mut recovered) = load_existing(&world).unwrap();
    assert_eq!(generation, expected_generation);
    let expected = if expected_generation == 1 {
        &previous
    } else {
        &replacement
    };
    assert_eq!(&recovered.checkpoint().unwrap(), expected);
    assert_eq!(
        load_generation(&world, 1).unwrap().checkpoint().unwrap(),
        previous
    );
    assert_eq!(
        load_generation(&world, 2).unwrap().checkpoint().unwrap(),
        replacement
    );

    recovered.step(vec![]);
    assert_eq!(save(&world, Some(generation), &recovered).unwrap(), 3);
    let (generation, newest) = load_existing(&world).unwrap();
    assert_eq!(generation, 3);
    assert_eq!(
        newest.checkpoint().unwrap(),
        recovered.checkpoint().unwrap()
    );
    assert_eq!(
        load_generation(&world, 2).unwrap().checkpoint().unwrap(),
        replacement
    );
}

#[test]
fn killing_writer_before_current_preserves_the_previous_generation() {
    assert_recovery_after_killing_writer("before_current", 1);
}

#[test]
fn killing_writer_after_current_preserves_the_new_generation() {
    assert_recovery_after_killing_writer("after_current", 2);
}
