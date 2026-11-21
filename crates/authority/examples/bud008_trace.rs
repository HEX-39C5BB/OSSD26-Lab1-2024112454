//! Export normalized BUD-008/009 dual-source semantics.
//!
//! Run from the repository root:
//! cargo run --offline --locked -p rustcraft-authority --example bud008_trace

use rustcraft_authority::java_bud::BudRecordKind;
use rustcraft_authority::java_signal::Face;
use rustcraft_authority::{Capabilities, Mutation, Request, RulePreset, Server, SessionId};
use rustcraft_world::{BlockPos, ChunkPos};

const PISTON: BlockPos = BlockPos::new(258, 110, 12);
const HEAD: BlockPos = BlockPos::new(259, 110, 12);
const QC: BlockPos = BlockPos::new(258, 111, 11);
const SOURCE_A: BlockPos = BlockPos::new(258, 109, 12);
const SOURCE_B: BlockPos = BlockPos::new(258, 111, 12);
const CHUNK: ChunkPos = ChunkPos { x: 16, z: 0 };

fn main() {
    for (scenario, order, repeat) in [
        ("a", vec![SOURCE_A], false),
        ("b", vec![SOURCE_B], false),
        ("ab", vec![SOURCE_A, SOURCE_B], false),
        ("ba", vec![SOURCE_B, SOURCE_A], false),
        ("a-repeat", vec![SOURCE_A], true),
        ("b-repeat", vec![SOURCE_B], true),
    ] {
        run(scenario, &order, repeat);
    }
}

fn run(scenario: &str, order: &[BlockPos], repeat: bool) {
    let mut fixture = Fixture::new();
    fixture.place(PISTON, "rustcraft_java:piston");
    fixture.place(QC, "rustcraft_java:redstone_block");
    fixture.world.watch_java_bud(PISTON);
    for position in order {
        fixture.place(*position, "rustcraft_java:glass");
        if repeat {
            fixture.place(*position, "rustcraft:empty");
            fixture.place(*position, "rustcraft_java:glass");
        }
    }

    let mut checks = Vec::new();
    let mut shapes = Vec::new();
    let mut events = Vec::new();
    for record in fixture.world.java_bud_trace() {
        match record.kind {
            BudRecordKind::Check {
                source: Some(source),
                ..
            } if source == SOURCE_A || source == SOURCE_B => {
                checks.push(format!(
                    "{{\"gt\":{},\"sequence\":{},\"source\":{},\"kind\":\"nc\"}}",
                    record.stamp.gt,
                    record.stamp.sequence,
                    position(source)
                ));
            }
            BudRecordKind::Shape { from } if from == Face::Down || from == Face::Up => {
                let direction = if from == Face::Down { "down" } else { "up" };
                shapes.push(format!(
                    "{{\"gt\":{},\"sequence\":{},\"direction\":\"{}\",\"kind\":\"shape\"}}",
                    record.stamp.gt, record.stamp.sequence, direction
                ));
            }
            BudRecordKind::BlockEvent(trace) if trace.event.base == PISTON => {
                events.push(format!(
                    "{{\"gt\":{},\"sequence\":{},\"stage\":\"{}\",\"action\":\"{}\",\"kind\":\"block_event\"}}",
                    record.stamp.gt,
                    record.stamp.sequence,
                    format!("{:?}", trace.stage).to_lowercase(),
                    format!("{:?}", trace.event.action).to_lowercase()
                ));
            }
            _ => {}
        }
    }
    let pending_events = fixture
        .world
        .pending_piston_events()
        .filter(|event| event.base == PISTON)
        .count();
    for _ in 0..5 {
        fixture.step();
    }
    let extended = fixture.state(PISTON) == "rustcraft_java:piston_extended";
    let head = fixture.state(HEAD) == "rustcraft_java:piston_head_east";
    println!(
        "{{\"scenario\":\"{scenario}\",\"checks\":[{}],\"shapes\":[{}],\"events\":[{}],\"pending_events\":{},\"final_extended\":{},\"final_head\":{}}}",
        checks.join(","),
        shapes.join(","),
        events.join(","),
        pending_events,
        extended,
        head
    );
}

struct Fixture {
    world: Server,
    session: SessionId,
    operation: u64,
}

impl Fixture {
    fn new() -> Self {
        let preset = RulePreset::JavaFlatV3;
        let mut world = Server::playable_with_rules(0, preset.default_range(), preset).unwrap();
        let session = world.open_local_session(Capabilities::P1_HOST);
        let mut fixture = Self {
            world,
            session,
            operation: 0,
        };
        fixture.request(Mutation::SetChunkActive {
            position: CHUNK,
            active: true,
        });
        fixture
    }

    fn request(&mut self, mutation: Mutation) {
        self.operation += 1;
        let report = self.world.step_local(
            vec![Request {
                session: self.session,
                operation: self.operation,
                mutation,
            }],
            false,
            false,
        );
        assert!(report.acks[0].result.is_ok(), "{:?}", report.acks);
    }

    fn place(&mut self, position: BlockPos, name: &str) {
        let state = self.world.registry().block_id(name).unwrap();
        let expected_revision = self.world.snapshot().chunks[&position.chunk()].revision();
        self.request(Mutation::SetBlock {
            position,
            state,
            expected_revision,
        });
    }

    fn step(&mut self) {
        self.world.step_local(Vec::new(), true, false);
    }

    fn state(&self, position: BlockPos) -> &str {
        let state = self.world.snapshot().chunks[&position.chunk()]
            .block(position)
            .unwrap();
        self.world.registry().block(state).unwrap().name
    }
}

fn position(position: BlockPos) -> String {
    format!("[{},{},{}]", position.x, position.y, position.z)
}
