use std::fs;
use std::path::Path;
use std::process::Command;

use rustcraft_authority::{
    Capabilities, Mutation, Request, RulePreset, ScheduledOperation, Server,
};
use rustcraft_world::BlockPos;
use serde_json::Value;

fn read_command(world: &Path, flag: &str, extra: &[&str]) -> Vec<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_rustcraft-server"))
        .arg(flag)
        .arg(world)
        .args(["9", "11", "9"])
        .args(extra)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn inspects_and_replays_quasi_power_from_a_checkpoint_without_publishing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path();
    let preset = RulePreset::JavaFlatV2;
    let mut server = Server::playable_with_rules(0, preset.default_range(), preset).unwrap();
    let session = server.open_local_session(Capabilities::P1_HOST);
    let base = BlockPos::new(9, 11, 9);
    let piston = server.registry().block_id("rustcraft_java:piston").unwrap();
    let expected_revision = server.snapshot().chunks[&base.chunk()].revision();
    let result = server.step_local(
        vec![Request {
            session,
            operation: 1,
            mutation: Mutation::SetBlock {
                position: base,
                state: piston,
                expected_revision,
            },
        }],
        false,
        false,
    );
    assert!(result.acks[0].result.is_ok());
    for (due, position, name) in [
        (1, BlockPos::new(9, 12, 8), "rustcraft_java:redstone_block"),
        (2, BlockPos::new(9, 10, 9), "rustcraft_java:glass"),
    ] {
        let expected = server.registry().block_id("rustcraft:empty").unwrap();
        let replacement = server.registry().block_id(name).unwrap();
        server
            .schedule_operation(
                due,
                ScheduledOperation::ReplaceBlock {
                    position,
                    expected,
                    replacement,
                },
            )
            .unwrap();
    }
    let generation = rustcraft_persistence::save(path, None, &server).unwrap();
    let manifest = path.join(format!("checkpoints/{generation}/manifest.json"));
    let original = fs::read(&manifest).unwrap();
    fs::remove_file(path.join(".lock")).unwrap();

    let inspected = read_command(path, "--inspect-bud", &[]);
    assert_eq!(inspected.len(), 2);
    assert_eq!(inspected[0]["ruleset"], "java/1.20.1");
    assert_eq!(inspected[1]["status"], "consistent");
    assert_eq!(inspected[1]["gt"], 0);
    assert!(inspected[1]["last_check"].is_null());

    let trace = read_command(path, "--trace-bud", &["5"]);
    let missing = trace
        .iter()
        .find(|event| event["details"]["status"] == "missing_update")
        .unwrap();
    assert_eq!(missing["stamp"]["gt"], 1);
    assert_eq!(missing["details"]["activation"]["kind"], "quasi");
    assert_eq!(
        missing["details"]["activation"]["source"],
        serde_json::json!([9, 12, 8])
    );
    let check = trace
        .iter()
        .find(|event| event["event"] == "activation_check")
        .unwrap();
    let queued = trace
        .iter()
        .find(|event| event["details"]["stage"] == "Queued")
        .unwrap();
    let started = trace
        .iter()
        .find(|event| event["details"]["stage"] == "Started")
        .unwrap();
    assert_eq!(queued["parent_sequence"], check["stamp"]["sequence"]);
    assert_eq!(started["parent_sequence"], queued["stamp"]["sequence"]);
    assert_eq!(started["stamp"]["phase"], "BlockEvent");
    assert_eq!(trace.last().unwrap()["status"], "consistent");
    assert_eq!(trace.last().unwrap()["gt"], 5);
    assert_eq!(read_command(path, "--trace-bud", &["5"]), trace);
    assert_eq!(fs::read(manifest).unwrap(), original);
    assert_eq!(fs::read(path.join("CURRENT")).unwrap(), b"1\n");
    assert_eq!(fs::read_dir(path.join("checkpoints")).unwrap().count(), 1);
    assert!(!path.join(".lock").exists());
}
