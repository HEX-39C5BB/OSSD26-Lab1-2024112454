use std::fs;
use std::io::ErrorKind;
use std::sync::Barrier;
use std::thread;

use rustcraft_authority::Server;
use rustcraft_persistence::{copy_checkpoint, load_existing, migrate_checkpoint, save};

#[test]
fn interrupted_publication_leaves_the_previous_checkpoint_readable() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let backup = root.path().join("backup");
    let mut server = Server::playable(42).unwrap();
    let previous = server.checkpoint().unwrap();
    assert_eq!(save(&world, None, &server).unwrap(), 1);
    server.step(vec![]);
    assert_eq!(save(&world, Some(1), &server).unwrap(), 2);

    // Model an interruption after the directory rename and before replacing CURRENT.
    let pending_pointer = world.join("pointer-before-rename");
    fs::rename(world.join("CURRENT"), &pending_pointer).unwrap();
    fs::write(world.join("CURRENT"), b"1\n").unwrap();
    let abandoned = world.join("checkpoints/.pending-interrupted");
    fs::create_dir(&abandoned).unwrap();
    fs::write(abandoned.join("player.cbor"), b"incomplete").unwrap();
    let unpublished_manifest = world.join("checkpoints/2/manifest.json");
    let unpublished = fs::read(&unpublished_manifest).unwrap();

    let (generation, loaded) = load_existing(&world).unwrap();
    assert_eq!(generation, 1);
    assert_eq!(loaded.checkpoint().unwrap(), previous);
    assert_eq!(copy_checkpoint(&world, &backup).unwrap(), 1);
    assert_eq!(
        load_existing(&backup).unwrap().1.checkpoint().unwrap(),
        previous
    );

    server.step(vec![]);
    assert_eq!(save(&world, Some(generation), &server).unwrap(), 3);
    let (generation, loaded) = load_existing(&world).unwrap();
    assert_eq!(generation, 3);
    assert_eq!(loaded.checkpoint().unwrap(), server.checkpoint().unwrap());
    assert_eq!(fs::read(unpublished_manifest).unwrap(), unpublished);
    assert_eq!(fs::read(pending_pointer).unwrap(), b"2\n");
}

#[test]
fn concurrent_writers_publish_once_for_the_same_lease() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path();
    let initial = Server::playable(42).unwrap();
    save(world, None, &initial).unwrap();
    let (generation, mut first) = load_existing(world).unwrap();
    let (_, mut second) = load_existing(world).unwrap();
    first.step(vec![]);
    second.step(vec![]);
    second.step(vec![]);
    let first_checkpoint = first.checkpoint().unwrap();
    let second_checkpoint = second.checkpoint().unwrap();
    let start = Barrier::new(2);

    let (a, b) = thread::scope(|threads| {
        let writer = |server: Server| {
            start.wait();
            save(world, Some(generation), &server)
        };
        let a = threads.spawn(move || writer(first));
        let b = threads.spawn(move || writer(second));
        (a.join().unwrap(), b.join().unwrap())
    });
    let (winner, error) = match (a, b) {
        (Ok(2), Err(error)) => (first_checkpoint, error),
        (Err(error), Ok(2)) => (second_checkpoint, error),
        results => panic!("expected one writer to win: {results:?}"),
    };
    assert!(error.to_string().contains("generation changed"));
    assert_eq!(fs::read_dir(world.join("checkpoints")).unwrap().count(), 2);
    let (generation, loaded) = load_existing(world).unwrap();
    assert_eq!(generation, 2);
    assert_eq!(loaded.checkpoint().unwrap(), winner);
}

#[test]
fn rejects_missing_truncated_and_corrupt_regions_before_publishing_a_copy() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let backup = root.path().join("backup");
    let bad_copy = root.path().join("bad-copy");
    let restored = root.path().join("restored");
    let mut server = Server::playable(42).unwrap();
    let previous = server.checkpoint().unwrap();
    save(&world, None, &server).unwrap();
    copy_checkpoint(&world, &backup).unwrap();
    server.step(vec![]);
    assert_eq!(save(&world, Some(1), &server).unwrap(), 2);
    let name = "r.0.0.rcr";
    let path = world.join("checkpoints/2").join(name);
    let original = fs::read(&path).unwrap();
    let mut corrupt = original.clone();
    *corrupt.last_mut().unwrap() ^= 1;

    for (record, kind, diagnostic) in [
        (None, ErrorKind::NotFound, "region"),
        (
            Some(&original[..original.len() - 1]),
            ErrorKind::InvalidData,
            "length does not match index",
        ),
        (
            Some(corrupt.as_slice()),
            ErrorKind::InvalidData,
            "CRC32C mismatch",
        ),
    ] {
        match record {
            None => fs::remove_file(&path).unwrap(),
            Some(bytes) => fs::write(&path, bytes).unwrap(),
        }
        let error = load_existing(&world).err().unwrap();
        assert_eq!(error.kind(), kind);
        assert!(
            error
                .to_string()
                .contains(&format!("checkpoint 2 region {name}"))
        );
        assert!(error.to_string().contains(diagnostic));
        for transfer in [copy_checkpoint, migrate_checkpoint] {
            let error = transfer(&world, &bad_copy).unwrap_err();
            assert_eq!(error.kind(), kind);
            assert!(error.to_string().contains(name));
            assert!(!bad_copy.exists());
        }
        assert_eq!(fs::read(world.join("CURRENT")).unwrap(), b"2\n");
    }

    assert_eq!(copy_checkpoint(&backup, &restored).unwrap(), 1);
    let (_, recovered) = load_existing(&restored).unwrap();
    assert_eq!(recovered.checkpoint().unwrap(), previous);
}

#[cfg(unix)]
#[test]
fn a_dangling_current_pointer_is_never_treated_as_a_new_world() {
    use rustcraft_persistence::load;
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let world = root.path();
    let pointer = world.join("CURRENT");
    symlink("missing-pointer-target", &pointer).unwrap();
    let error = load(world).err().unwrap();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert!(
        error
            .to_string()
            .contains("CURRENT points to a missing file")
    );
    let server = Server::playable(42).unwrap();
    assert_eq!(
        save(world, None, &server).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(
        pointer.read_link().unwrap(),
        std::path::Path::new("missing-pointer-target")
    );
    assert!(!world.join("checkpoints").exists());
}
