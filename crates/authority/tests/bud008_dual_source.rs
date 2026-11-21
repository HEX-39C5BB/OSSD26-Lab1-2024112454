use rustcraft_authority::java_bud::BudRecordKind;
use rustcraft_authority::java_signal::Face;
use rustcraft_authority::{
    Capabilities, JavaNotification, Mutation, Request, RulePreset, Server, SessionId,
};
use rustcraft_world::{BlockPos, ChunkPos};

const PISTON: BlockPos = BlockPos::new(258, 110, 12);
const QC: BlockPos = BlockPos::new(258, 111, 11);
const SOURCE_A: BlockPos = BlockPos::new(258, 109, 12);
const SOURCE_B: BlockPos = BlockPos::new(258, 111, 12);
const CHUNK: ChunkPos = ChunkPos { x: 16, z: 0 };

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

#[test]
fn dual_source_nc_shape_cascades_queue_one_piston_event() {
    for (name, order, repeat) in [
        ("a", vec![SOURCE_A], false),
        ("b", vec![SOURCE_B], false),
        ("ab", vec![SOURCE_A, SOURCE_B], false),
        ("ba", vec![SOURCE_B, SOURCE_A], false),
        ("a-repeat", vec![SOURCE_A], true),
        ("b-repeat", vec![SOURCE_B], true),
    ] {
        let mut fixture = Fixture::new();
        fixture.place(PISTON, "rustcraft_java:piston");
        fixture.place(QC, "rustcraft_java:redstone_block");
        fixture.world.watch_java_bud(PISTON);
        assert_eq!(
            fixture
                .world
                .pending_piston_events()
                .filter(|event| event.base == PISTON)
                .count(),
            0,
            "{name}: QC alone queued an event"
        );

        for position in &order {
            fixture.place(*position, "rustcraft_java:glass");
            if repeat {
                fixture.place(*position, "rustcraft:empty");
                fixture.place(*position, "rustcraft_java:glass");
            }
        }
        let events: Vec<_> = fixture
            .world
            .pending_piston_events()
            .filter(|event| event.base == PISTON)
            .collect();
        assert_eq!(events.len(), 1, "{name}: expected one queued event");
        let stamps: Vec<_> = fixture
            .world
            .java_bud_trace()
            .filter_map(|record| match record.kind {
                BudRecordKind::Check {
                    source: Some(source),
                    ..
                } if source == SOURCE_A || source == SOURCE_B => Some((source, record.stamp.gt)),
                _ => None,
            })
            .collect();
        let sources: Vec<_> = stamps.iter().map(|(source, _)| *source).collect();
        let expected_checks = if repeat {
            vec![order[0], order[0], order[0]]
        } else {
            order.clone()
        };
        assert_eq!(sources, expected_checks, "{name}: check order");
        assert!(
            stamps.iter().all(|(_, gt)| *gt == stamps[0].1),
            "{name}: source checks were not in one GT: {stamps:?}"
        );
        let shapes: Vec<_> = fixture
            .world
            .java_bud_trace()
            .filter_map(|record| match record.kind {
                BudRecordKind::Shape { from } => Some((from, record.stamp.gt)),
                _ => None,
            })
            .collect();
        let expected_shapes: Vec<Face> = order
            .iter()
            .map(|position| {
                if *position == SOURCE_A {
                    Face::Down
                } else {
                    Face::Up
                }
            })
            .collect();
        let expected_shapes = if repeat {
            vec![expected_shapes[0]; 3]
        } else {
            expected_shapes
        };
        assert_eq!(
            shapes.iter().filter(|(_, gt)| *gt == stamps[0].1).count(),
            expected_shapes.len(),
            "{name}: duplicate Shape count"
        );
        for expected in expected_shapes {
            assert!(
                shapes
                    .iter()
                    .any(|(from, gt)| *from == expected && *gt == stamps[0].1),
                "{name}: missing shape {expected:?} in one GT: {shapes:?}"
            );
        }
        let event_stages: Vec<_> = fixture
            .world
            .java_bud_trace()
            .filter_map(|record| match record.kind {
                BudRecordKind::BlockEvent(trace) if trace.event.base == PISTON => Some(trace.stage),
                _ => None,
            })
            .collect();
        assert_eq!(
            event_stages
                .iter()
                .filter(|stage| **stage == rustcraft_authority::PistonTraceStage::Queued)
                .count(),
            1,
            "{name}: expected exactly one queued BlockEvent: {event_stages:?}"
        );
        if repeat {
            assert_eq!(
                event_stages
                    .iter()
                    .filter(|stage| **stage == rustcraft_authority::PistonTraceStage::Duplicate)
                    .count(),
                2,
                "{name}: repeated notices should deduplicate twice: {event_stages:?}"
            );
        }

        for _ in 0..5 {
            fixture.step();
        }
        assert_eq!(
            fixture.state(PISTON),
            "rustcraft_java:piston_extended",
            "{name}: piston did not settle extended"
        );
        assert_eq!(
            fixture
                .world
                .pending_piston_events()
                .filter(|event| event.base == PISTON)
                .count(),
            0,
            "{name}: event queue did not clear"
        );
    }
}

#[test]
fn shape_updates_from_all_faces_do_not_activate_but_neighbor_changes_do() {
    for face in [
        Face::West,
        Face::East,
        Face::Down,
        Face::Up,
        Face::North,
        Face::South,
    ] {
        let mut fixture = Fixture::new();
        let source = face.offset(PISTON).unwrap();
        fixture.place(source, "rustcraft_java:glass");
        fixture.place(PISTON, "rustcraft_java:piston");
        fixture.place(QC, "rustcraft_java:redstone_block");
        fixture.world.watch_java_bud(PISTON);

        fixture.request(Mutation::NotifyJavaBlock {
            target: PISTON,
            from: face,
            kind: JavaNotification::ShapeUpdate,
        });
        assert_eq!(
            fixture
                .world
                .pending_piston_events()
                .filter(|event| event.base == PISTON)
                .count(),
            0,
            "{face:?}: a Shape update must not enqueue the QC activation"
        );
        assert!(
            fixture.world.java_bud_trace().any(|record| matches!(
                record.kind,
                BudRecordKind::Shape { from } if from == face
            )),
            "{face:?}: expected the directional Shape record"
        );
        for _ in 0..2 {
            fixture.step();
        }
        assert_eq!(fixture.state(PISTON), "rustcraft_java:piston");

        fixture.request(Mutation::NotifyJavaBlock {
            target: PISTON,
            from: face,
            kind: JavaNotification::NeighborChanged,
        });
        let pending: Vec<_> = fixture
            .world
            .pending_piston_events()
            .filter(|event| event.base == PISTON)
            .collect();
        assert_eq!(pending.len(), 1, "{face:?}: NC should enqueue once");
        for _ in 0..5 {
            fixture.step();
        }
        assert_eq!(
            fixture.state(PISTON),
            "rustcraft_java:piston_extended",
            "{face:?}: NC should activate the powered piston"
        );
    }
}

#[test]
fn neighbor_exclusion_dispatch_notifies_every_face_except_the_selected_one() {
    let source = BlockPos::new(258, 110, 12);
    for excluded in [
        Face::West,
        Face::East,
        Face::Down,
        Face::Up,
        Face::North,
        Face::South,
    ] {
        for target_face in [
            Face::West,
            Face::East,
            Face::Down,
            Face::Up,
            Face::North,
            Face::South,
        ] {
            let mut fixture = Fixture::new();
            let target = target_face.offset(source).unwrap();
            fixture.place(source, "rustcraft_java:glass");
            fixture.place(target, "rustcraft_java:piston");
            fixture.world.watch_java_bud(target);
            fixture.request(Mutation::NotifyJavaNeighborsExcept { source, excluded });

            let checks: Vec<_> = fixture
                .world
                .java_bud_trace()
                .filter_map(|record| match record.kind {
                    BudRecordKind::Check {
                        source: Some(actual),
                        ..
                    } => Some(actual),
                    _ => None,
                })
                .collect();
            let expected = (target_face != excluded)
                .then_some(source)
                .into_iter()
                .collect::<Vec<_>>();
            assert_eq!(
                checks, expected,
                "excluded={excluded:?}, target={target_face:?}"
            );
        }
    }
}
