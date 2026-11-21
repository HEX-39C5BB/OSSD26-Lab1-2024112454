//! Emit the RustCraft half of the BUD-013 online-chunk queue comparison.
//!
//! Run from the repository root with:
//! cargo run --offline --locked -p rustcraft-authority --example bud013_trace

use rustcraft_authority::{Capabilities, Effect, Mutation, Request, RulePreset, Server};
use rustcraft_world::{BlockPos, ChunkPos};

fn main() {
    let preset = RulePreset::JavaFlatV3;
    let mut server = Server::playable_with_rules(42, preset.default_range(), preset).unwrap();
    let session = server.open_local_session(Capabilities::P1_HOST);
    let chunk = ChunkPos { x: 16, z: 0 };
    let mut operation = 0;

    set_active(&mut server, session, &mut operation, chunk, true);
    lamp(&mut server, session, &mut operation, chunk);
    piston(&mut server, session, &mut operation, chunk);
    section(&mut server, session, &mut operation, chunk);
    chain(&mut server, session, &mut operation, chunk);
}

fn chain(
    server: &mut Server,
    session: rustcraft_authority::SessionId,
    operation: &mut u64,
    chunk: ChunkPos,
) {
    let probes: Vec<BlockPos> = (264..269).map(|x| BlockPos::new(x, 110, 2)).collect();
    let heads: Vec<BlockPos> = (264..269).map(|x| BlockPos::new(x, 110, 3)).collect();
    for x in 264..269 {
        put(
            server,
            session,
            operation,
            BlockPos::new(x, 109, 1),
            "rustcraft_java:smooth_stone",
        );
        put(
            server,
            session,
            operation,
            BlockPos::new(x, 110, 1),
            "rustcraft_java:wire_0_30",
        );
        put(
            server,
            session,
            operation,
            BlockPos::new(x, 110, 2),
            "rustcraft_java:piston_south",
        );
    }
    for _ in 0..2 {
        server.step(Vec::new());
    }
    for x in 264..269 {
        put(
            server,
            session,
            operation,
            BlockPos::new(x, 111, 3),
            "rustcraft_java:redstone_block",
        );
    }
    for _ in 0..2 {
        server.step(Vec::new());
    }
    sample(server, "DEDICATED-BUD013-CHAIN", "ready");
    put_and_deactivate(
        server,
        session,
        operation,
        BlockPos::new(263, 110, 1),
        "rustcraft_java:redstone_block",
        chunk,
    );
    sample(server, "DEDICATED-BUD013-CHAIN", "away");
    for _ in 0..2 {
        server.step(Vec::new());
    }
    set_active(server, session, operation, chunk, true);
    for _ in 0..2 {
        server.step(Vec::new());
    }
    sample(server, "DEDICATED-BUD013-CHAIN", "after_return");
    let extended = probes
        .iter()
        .filter(|position| state(server, **position) == "rustcraft_java:piston_south_extended")
        .count();
    let heads = heads
        .iter()
        .filter(|position| state(server, **position) == "rustcraft_java:piston_head_south")
        .count();
    println!(
        "{{\"fixture\":\"DEDICATED-BUD013-CHAIN\",\"phase\":\"final\",\"extended\":{extended},\"heads\":{heads}}}"
    );
}

fn lamp(
    server: &mut Server,
    session: rustcraft_authority::SessionId,
    operation: &mut u64,
    chunk: ChunkPos,
) {
    let lamp = BlockPos::new(264, 110, 6);
    let power = BlockPos::new(263, 110, 6);
    put(
        server,
        session,
        operation,
        lamp,
        "rustcraft_java:redstone_lamp",
    );
    put(
        server,
        session,
        operation,
        power,
        "rustcraft_java:redstone_block",
    );
    sample(server, "DEDICATED-BUD013-LAMP", "powered");
    put(server, session, operation, power, "rustcraft:empty");
    sample(server, "DEDICATED-BUD013-LAMP", "before_away");
    set_active(server, session, operation, chunk, false);
    for _ in 0..5 {
        server.step(Vec::new());
    }
    sample(server, "DEDICATED-BUD013-LAMP", "away");
    set_active(server, session, operation, chunk, true);
    server.step(Vec::new());
    sample(server, "DEDICATED-BUD013-LAMP", "after_return");
    println!(
        "{{\"fixture\":\"DEDICATED-BUD013-LAMP\",\"phase\":\"final\",\"lit\":{}}}",
        state(server, lamp) == "rustcraft_java:redstone_lamp_on"
    );
}

fn piston(
    server: &mut Server,
    session: rustcraft_authority::SessionId,
    operation: &mut u64,
    chunk: ChunkPos,
) {
    let piston = BlockPos::new(264, 110, 10);
    let head = BlockPos::new(265, 110, 10);
    let load = BlockPos::new(266, 110, 10);
    let power = BlockPos::new(263, 110, 10);
    put(server, session, operation, piston, "rustcraft_java:piston");
    put(
        server,
        session,
        operation,
        load,
        "rustcraft_java:smooth_stone",
    );
    put_and_deactivate(
        server,
        session,
        operation,
        power,
        "rustcraft_java:redstone_block",
        chunk,
    );
    sample(server, "DEDICATED-BUD013-PISTON", "away");
    for _ in 0..2 {
        server.step(Vec::new());
    }
    set_active(server, session, operation, chunk, true);
    for _ in 0..2 {
        server.step(Vec::new());
    }
    sample(server, "DEDICATED-BUD013-PISTON", "after_return");
    println!(
        "{{\"fixture\":\"DEDICATED-BUD013-PISTON\",\"phase\":\"final\",\"extended\":{},\"head\":{},\"load\":{}}}",
        state(server, piston) == "rustcraft_java:piston_extended",
        state(server, head) == "rustcraft_java:piston_head_east",
        state(server, load) == "rustcraft_java:smooth_stone"
    );
}

fn section(
    server: &mut Server,
    session: rustcraft_authority::SessionId,
    operation: &mut u64,
    chunk: ChunkPos,
) {
    let piston = BlockPos::new(264, 15, 6);
    let head = BlockPos::new(264, 16, 6);
    let load = BlockPos::new(264, 17, 6);
    let power = BlockPos::new(263, 15, 6);
    put(
        server,
        session,
        operation,
        piston,
        "rustcraft_java:piston_up",
    );
    put(
        server,
        session,
        operation,
        load,
        "rustcraft_java:smooth_stone",
    );
    put_and_deactivate(
        server,
        session,
        operation,
        power,
        "rustcraft_java:redstone_block",
        chunk,
    );
    sample(server, "DEDICATED-BUD013-SECTION", "away");
    server.step(Vec::new());
    set_active(server, session, operation, chunk, true);
    for _ in 0..2 {
        server.step(Vec::new());
    }
    sample(server, "DEDICATED-BUD013-SECTION", "after_return");
    println!(
        "{{\"fixture\":\"DEDICATED-BUD013-SECTION\",\"phase\":\"final\",\"extended\":{},\"head\":{},\"load\":{}}}",
        state(server, piston) == "rustcraft_java:piston_up_extended",
        state(server, head) == "rustcraft_java:piston_head_up",
        state(server, load) == "rustcraft_java:smooth_stone"
    );
}

fn put(
    server: &mut Server,
    session: rustcraft_authority::SessionId,
    operation: &mut u64,
    position: BlockPos,
    name: &str,
) {
    *operation += 1;
    let expected_revision = server.snapshot().chunks[&position.chunk()].revision();
    let report = server.step(vec![Request {
        session,
        operation: *operation,
        mutation: Mutation::SetBlock {
            position,
            expected_revision,
            state: server.registry().block_id(name).unwrap(),
        },
    }]);
    assert_eq!(
        report.acks[0].result,
        Ok(Effect::ChunkChanged(position.chunk()))
    );
}

fn put_and_deactivate(
    server: &mut Server,
    session: rustcraft_authority::SessionId,
    operation: &mut u64,
    position: BlockPos,
    name: &str,
    chunk: ChunkPos,
) {
    *operation += 1;
    let set = Request {
        session,
        operation: *operation,
        mutation: Mutation::SetBlock {
            position,
            expected_revision: server.snapshot().chunks[&position.chunk()].revision(),
            state: server.registry().block_id(name).unwrap(),
        },
    };
    *operation += 1;
    let deactivate = Request {
        session,
        operation: *operation,
        mutation: Mutation::SetChunkActive {
            position: chunk,
            active: false,
        },
    };
    let report = server.step(vec![set, deactivate]);
    assert!(report.acks.iter().all(|ack| ack.result.is_ok()));
}

fn set_active(
    server: &mut Server,
    session: rustcraft_authority::SessionId,
    operation: &mut u64,
    chunk: ChunkPos,
    active: bool,
) {
    *operation += 1;
    let report = server.step(vec![Request {
        session,
        operation: *operation,
        mutation: Mutation::SetChunkActive {
            position: chunk,
            active,
        },
    }]);
    assert!(report.acks[0].result.is_ok());
}

fn state(server: &Server, position: BlockPos) -> &'static str {
    let id = server.snapshot().chunks[&position.chunk()]
        .block(position)
        .unwrap();
    server.registry().block(id).unwrap().name
}

fn sample(server: &Server, fixture: &str, phase: &str) {
    let ticks: Vec<String> = server
        .pending_java_block_ticks()
        .map(|tick| {
            format!(
                "{{\"pos\":[{},{},{}],\"kind\":\"{:?}\",\"due\":{}}}",
                tick.position.x, tick.position.y, tick.position.z, tick.kind, tick.due_gt
            )
        })
        .collect();
    let events: Vec<String> = server
        .pending_piston_events()
        .map(|event| {
            format!(
                "{{\"pos\":[{},{},{}],\"action\":\"{:?}\",\"kind\":\"{:?}\"}}",
                event.base.x, event.base.y, event.base.z, event.action, event.kind
            )
        })
        .collect();
    println!(
        "{{\"gt\":{},\"fixture\":\"{fixture}\",\"phase\":\"{phase}\",\"block_ticks\":[{}],\"block_events\":[{}]}}",
        server.snapshot().tick,
        ticks.join(","),
        events.join(",")
    );
}
