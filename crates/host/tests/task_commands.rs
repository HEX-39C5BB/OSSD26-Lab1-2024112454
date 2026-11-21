use std::fs;
use std::path::Path;
use std::process::Command;

use rustcraft_authority::{ScheduledOperation, Server};
use rustcraft_persistence::{load_existing, load_generation, save};
use rustcraft_world::{BlockPos, ChunkPos};

fn command(flag: &str, world: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rustcraft-server"));
    command.arg(flag).arg(world);
    command
}

#[test]
fn lists_pending_tasks_in_due_order_without_writing_or_advancing_the_world() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path();
    let mut server = Server::playable(42).unwrap();
    server.step(vec![]);
    let empty = server.registry().block_id("rustcraft:empty").unwrap();
    let basalt = server
        .registry()
        .block_id("rustcraft_content:basalt")
        .unwrap();
    server
        .schedule_operation(
            8,
            ScheduledOperation::ReplaceBlock {
                position: BlockPos::new(2, 0, -3),
                expected: empty,
                replacement: basalt,
            },
        )
        .unwrap();
    let canceled = server.schedule_task(1).unwrap();
    assert!(server.cancel_task(canceled));
    server
        .schedule_operation(
            3,
            ScheduledOperation::DeactivateChunk {
                position: ChunkPos { x: -1, z: 0 },
            },
        )
        .unwrap();
    server.schedule_task(3).unwrap();
    assert_eq!(save(world, None, &server).unwrap(), 1);
    let manifest = world.join("checkpoints/1/manifest.json");
    let original = fs::read(&manifest).unwrap();
    fs::remove_file(world.join(".lock")).unwrap();

    let output = command("--list-tasks", world).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        [
            "event=scheduled_tasks generation=1 tick=1 pending=3",
            "event=scheduled_task id=3 due_tick=4 remaining_ticks=3 kind=deactivate_chunk chunk_x=-1 chunk_z=0",
            "event=scheduled_task id=4 due_tick=4 remaining_ticks=3 kind=event",
            "event=scheduled_task id=1 due_tick=9 remaining_ticks=8 kind=replace_block x=2 y=0 z=-3 expected=\"rustcraft:empty\" replacement=\"rustcraft_content:basalt\"",
        ]
    );
    assert_eq!(fs::read(manifest).unwrap(), original);
    assert_eq!(fs::read(world.join("CURRENT")).unwrap(), b"1\n");
    assert!(!world.join(".lock").exists());
    assert_eq!(fs::read_dir(world.join("checkpoints")).unwrap().count(), 1);
}

fn successful_output(flag: &str, world: &Path, args: &[&str]) -> String {
    let output = command(flag, world).args(args).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn offline_edits_publish_without_ticking_and_replay_after_restart() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path();
    let mut continuous = Server::playable(42).unwrap();
    let initial = continuous.checkpoint().unwrap();
    save(world, None, &continuous).unwrap();
    let saved_manifest = fs::read(world.join("checkpoints/1/manifest.json")).unwrap();

    for expected_generation in [2, 3] {
        let stdout = successful_output(
            "--schedule-replace-block",
            world,
            &[
                "2",
                "2",
                "0",
                "2",
                "rustcraft:empty",
                "rustcraft_content:basalt",
            ],
        );
        assert!(stdout.contains(&format!(
            "event=task_queued generation={expected_generation} id={} due_tick=2",
            expected_generation - 1
        )));
    }
    assert_eq!(
        successful_output("--schedule-deactivate-chunk", world, &["2", "-1", "0"]).trim(),
        "event=task_queued generation=4 id=3 due_tick=2"
    );
    assert_eq!(
        successful_output("--schedule-deactivate-chunk", world, &["1", "0", "0"]).trim(),
        "event=task_queued generation=5 id=4 due_tick=1"
    );
    assert_eq!(
        successful_output("--cancel-task", world, &["4"]).trim(),
        "event=task_canceled generation=6 id=4 due_tick=1"
    );

    let empty = continuous.registry().block_id("rustcraft:empty").unwrap();
    let basalt = continuous
        .registry()
        .block_id("rustcraft_content:basalt")
        .unwrap();
    let position = BlockPos::new(2, 0, 2);
    let replace = ScheduledOperation::ReplaceBlock {
        position,
        expected: empty,
        replacement: basalt,
    };
    continuous.schedule_operation(2, replace).unwrap();
    continuous.schedule_operation(2, replace).unwrap();
    let chunk = ChunkPos { x: -1, z: 0 };
    continuous
        .schedule_operation(2, ScheduledOperation::DeactivateChunk { position: chunk })
        .unwrap();
    let canceled = continuous
        .schedule_operation(
            1,
            ScheduledOperation::DeactivateChunk {
                position: ChunkPos::ORIGIN,
            },
        )
        .unwrap();
    assert!(continuous.cancel_task(canceled));
    let (generation, pending) = load_existing(world).unwrap();
    assert_eq!(generation, 6);
    assert_eq!(
        pending.checkpoint().unwrap(),
        continuous.checkpoint().unwrap()
    );
    assert_eq!(pending.snapshot().tick, 0);

    let output = command("--open-world", world)
        .args(["--ticks", "3"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("event=task_due tick=2 id=1 due_tick=2 kind=replace_block"));
    assert!(stderr.contains(
        "event=task_rejected tick=2 id=2 due_tick=2 kind=replace_block reason=UnexpectedBlockState"
    ));
    assert!(stderr.contains("event=task_due tick=2 id=3 due_tick=2 kind=deactivate_chunk"));
    assert!(!stderr.contains("id=4"));
    continuous.configure_game_mode(None);
    for _ in 0..3 {
        continuous.step(vec![]);
    }
    let (generation, restored) = load_existing(world).unwrap();
    assert_eq!(generation, 7);
    assert_eq!(
        restored.checkpoint().unwrap(),
        continuous.checkpoint().unwrap()
    );
    assert_eq!(
        restored.snapshot().chunks[&position.chunk()].block(position),
        Some(basalt)
    );
    assert!(!restored.snapshot().chunks.contains_key(&chunk));
    assert_eq!(restored.scheduled_tasks().len(), 0);
    assert_eq!(
        fs::read(world.join("checkpoints/1/manifest.json")).unwrap(),
        saved_manifest
    );
    assert_eq!(
        load_generation(world, 1).unwrap().checkpoint().unwrap(),
        initial
    );
}

#[test]
fn rejected_offline_edits_leave_the_source_generation_intact() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let server = Server::playable(42).unwrap();
    save(&world, None, &server).unwrap();
    let manifest = world.join("checkpoints/1/manifest.json");
    let initial = fs::read(&manifest).unwrap();
    let cases: [(&str, &[&str], &str); 4] = [
        (
            "--schedule-deactivate-chunk",
            &["0", "0", "0"],
            "invalid DELAY_TICKS",
        ),
        (
            "--schedule-replace-block",
            &["1", "0", "0", "0", "unknown:state", "rustcraft:empty"],
            "unknown block state",
        ),
        (
            "--schedule-replace-block",
            &[
                "1",
                "0",
                "256",
                "0",
                "rustcraft:empty",
                "rustcraft_content:basalt",
            ],
            "outside the world",
        ),
        ("--cancel-task", &["9"], "no pending task with id 9"),
    ];
    for (flag, args, error) in cases {
        let output = command(flag, &world).args(args).output().unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(String::from_utf8(output.stderr).unwrap().contains(error));
    }
    assert_eq!(fs::read(manifest).unwrap(), initial);
    assert_eq!(fs::read(world.join("CURRENT")).unwrap(), b"1\n");
    assert_eq!(fs::read_dir(world.join("checkpoints")).unwrap().count(), 1);

    let absent = root.path().join("absent");
    let output = command("--schedule-deactivate-chunk", &absent)
        .args(["1", "0", "0"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("no published checkpoint")
    );
    assert!(!absent.exists());
}
