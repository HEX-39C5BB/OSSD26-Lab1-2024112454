use std::process::Command;

#[test]
fn emits_local_task_diagnostics_for_each_outcome() {
    let output = Command::new(env!("CARGO_BIN_EXE_rustcraft-server"))
        .arg("--task-demo")
        .output()
        .unwrap();
    let diagnostics = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{diagnostics}");
    let events: Vec<_> = diagnostics
        .lines()
        .filter(|line| line.starts_with("event=task_"))
        .collect();
    assert_eq!(events.len(), 11, "{diagnostics}");
    assert!(events[0].starts_with("event=task_queued tick=1 id=1 due_tick=2 kind=replace_block"));
    assert_eq!(
        events[5],
        "event=task_canceled tick=1 id=5 due_tick=2 kind=event"
    );
    assert!(events[6].starts_with(
        "event=task_due tick=2 id=1 due_tick=2 kind=replace_block effect=ChunkChanged"
    ));
    assert!(events[7].starts_with(
        "event=task_rejected tick=2 id=2 due_tick=2 kind=replace_block reason=UnexpectedBlockState"
    ));
    assert!(events[8].starts_with(
        "event=task_due tick=2 id=3 due_tick=2 kind=deactivate_chunk effect=ChunkChanged"
    ));
    assert_eq!(
        events[9],
        "event=task_rejected tick=2 id=4 due_tick=2 kind=deactivate_chunk reason=ChunkOccupied"
    );
    assert_eq!(events[10], "event=task_demo_complete");
}
