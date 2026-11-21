use rustcraft_authority::java_bud::{
    BudComponent, BudPhase, BudRecordKind, BudStatus, EjectorOutcome, PistonObstruction,
};
use rustcraft_authority::java_signal::{ActivationPower, Face};
use rustcraft_authority::{
    Capabilities, JavaNotification, Mutation, PistonTraceStage, Request, RulePreset, Server,
    SessionId,
};
use rustcraft_world::{BlockPos, ChunkPos};

const BASE: BlockPos = BlockPos::new(9, 11, 9);
const QC: BlockPos = BlockPos::new(9, 12, 8);

struct Fixture {
    world: Server,
    session: SessionId,
    operation: u64,
}

impl Fixture {
    fn new(block: &str) -> Self {
        let rules = RulePreset::JavaFlatV2;
        let mut world = Server::playable_with_rules(0, rules.default_range(), rules).unwrap();
        let session = world.open_local_session(Capabilities::P1_HOST);
        let mut fixture = Self {
            world,
            session,
            operation: 0,
        };
        fixture.place(BASE, block);
        fixture
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

    fn notify(&mut self, kind: JavaNotification) {
        self.request(Mutation::NotifyJavaBlock {
            target: BASE,
            from: Face::Down,
            kind,
        });
    }

    fn step(&mut self) {
        self.world.step_local(Vec::new(), true, false);
    }
}

#[test]
fn watched_pistons_distinguish_missing_neighbor_notice_from_motion_and_queued_actions() {
    for block in ["rustcraft_java:piston", "rustcraft_java:sticky_piston"] {
        let mut fixture = Fixture::new(block);
        fixture.world.watch_java_bud(BASE);
        assert_eq!(
            fixture.world.inspect_java_bud(BASE).status,
            BudStatus::Consistent
        );
        fixture.place(QC, "rustcraft_java:redstone_block");
        let before_read = fixture.world.checkpoint().unwrap();
        let report = fixture.world.inspect_java_bud(BASE);
        assert_eq!(report.status, BudStatus::MissingUpdate);
        assert_eq!(report.activation, Some(ActivationPower::Quasi(QC)));
        assert_eq!(report.missing_since.unwrap().phase, BudPhase::Commit);
        assert_eq!(report.last_check, None);
        assert_eq!(fixture.world.checkpoint().unwrap(), before_read);
        fixture.notify(JavaNotification::ShapeUpdate);
        assert_eq!(
            fixture.world.inspect_java_bud(BASE).status,
            BudStatus::MissingUpdate
        );
        assert!(
            fixture
                .world
                .java_bud_trace()
                .any(|record| matches!(record.kind, BudRecordKind::Shape { from: Face::Down }))
        );

        fixture.notify(JavaNotification::NeighborChanged);
        let report = fixture.world.inspect_java_bud(BASE);
        assert_eq!(report.status, BudStatus::Pending);
        assert_eq!(report.piston_events.len(), 1);
        let queued = report.last_block_event.unwrap();
        assert_eq!(
            queued.parent_sequence,
            Some(report.last_check.unwrap().stamp.sequence)
        );
        fixture.step();
        let moving = fixture.world.inspect_java_bud(BASE);
        assert_eq!(moving.status, BudStatus::Moving);
        assert_eq!(moving.motion[0].remaining, 2);
        assert_eq!(
            moving.last_block_event.unwrap().stamp.phase,
            BudPhase::BlockEvent
        );
        fixture.step();
        fixture.step();
        let finished = fixture.world.inspect_java_bud(BASE);
        assert_eq!(finished.status, BudStatus::Consistent);
        assert!(matches!(finished.last_block_event.unwrap().kind,
            BudRecordKind::BlockEvent(trace) if trace.stage == PistonTraceStage::Completed));
        fixture.place(QC, "rustcraft:empty");
        let stale = fixture.world.inspect_java_bud(BASE);
        assert_eq!(stale.status, BudStatus::MissingUpdate);
        assert_eq!(stale.target.unwrap().active, Some(true));
        assert_eq!(stale.activation, Some(ActivationPower::Unpowered));

        let restored = Server::restore(fixture.world.checkpoint().unwrap()).unwrap();
        let offline = restored.inspect_java_bud(BASE);
        assert_eq!(offline.status, BudStatus::HistoryInsufficient);
        assert_eq!(offline.last_check, None);
        assert_eq!(offline.missing_since, None);
    }
}

#[test]
fn a_powered_piston_blocked_by_bedrock_has_a_concrete_obstruction() {
    let mut fixture = Fixture::new("rustcraft_java:piston");
    let head = Face::East.offset(BASE).unwrap();
    fixture.place(head, "rustcraft_java:bedrock");
    fixture.world.watch_java_bud(BASE);
    fixture.place(QC, "rustcraft_java:redstone_block");
    let blocked = fixture.world.inspect_java_bud(BASE);
    assert_eq!(blocked.status, BudStatus::Blocked);
    assert_eq!(
        blocked.obstruction,
        Some(PistonObstruction::Immovable {
            position: head,
            state: Some("rustcraft_java:bedrock"),
        })
    );
    assert!(blocked.missing_since.is_none());
    fixture.notify(JavaNotification::NeighborChanged);
    let checked = fixture.world.inspect_java_bud(BASE);
    assert_eq!(checked.status, BudStatus::Blocked);
    assert!(checked.last_check.is_some());
    assert!(checked.piston_events.is_empty());
}

#[test]
fn an_ejector_can_be_unpowered_with_a_pending_callback() {
    for (name, component) in [
        ("rustcraft_java:dispenser", BudComponent::Dispenser),
        ("rustcraft_java:dropper", BudComponent::Dropper),
    ] {
        let mut fixture = Fixture::new(name);
        fixture.world.watch_java_bud(BASE);
        fixture.place(QC, "rustcraft_java:redstone_block");
        let waiting = fixture.world.inspect_java_bud(BASE);
        assert_eq!(waiting.target.unwrap().component, component);
        assert_eq!(waiting.status, BudStatus::MissingUpdate);
        assert_eq!(waiting.activation, Some(ActivationPower::Quasi(QC)));
        fixture.notify(JavaNotification::NeighborChanged);
        let scheduled = fixture.world.inspect_java_bud(BASE);
        let insertion = scheduled.last_scheduled_event.unwrap();
        assert_eq!(
            insertion.parent_sequence,
            Some(scheduled.last_check.unwrap().stamp.sequence)
        );
        assert_eq!(scheduled.scheduled_ticks[0].due_gt, 4);
        fixture.place(QC, "rustcraft:empty");
        fixture.notify(JavaNotification::NeighborChanged);
        let unpowered = fixture.world.inspect_java_bud(BASE);
        assert_eq!(unpowered.status, BudStatus::Pending);
        assert_eq!(unpowered.target.unwrap().active, Some(false));
        assert_eq!(unpowered.activation, Some(ActivationPower::Unpowered));
        for _ in 0..4 {
            fixture.step();
        }
        let finished = fixture.world.inspect_java_bud(BASE);
        assert_eq!(finished.status, BudStatus::Consistent);
        let callback = finished.last_scheduled_event.unwrap();
        assert_eq!(callback.stamp.gt, 4);
        assert_eq!(callback.stamp.phase, BudPhase::ScheduledBlockTick);
        assert_eq!(callback.parent_sequence, Some(insertion.stamp.sequence));
        assert!(matches!(
            callback.kind,
            BudRecordKind::Callback {
                outcome: EjectorOutcome::Empty,
                ..
            }
        ));
    }
}

#[test]
fn removing_and_replacing_the_watched_block_resets_diagnostic_history() {
    let mut fixture = Fixture::new("rustcraft_java:dropper");
    fixture.world.watch_java_bud(BASE);
    fixture.place(QC, "rustcraft_java:redstone_block");
    assert_eq!(
        fixture.world.inspect_java_bud(BASE).status,
        BudStatus::MissingUpdate
    );
    fixture.place(BASE, "rustcraft:empty");
    fixture.place(BASE, "rustcraft_java:dropper");
    let replacement = fixture.world.inspect_java_bud(BASE);
    assert_eq!(replacement.status, BudStatus::HistoryInsufficient);
    assert_eq!(replacement.missing_since, None);
}

#[test]
fn a_missing_update_result_expires_when_its_observation_leaves_the_trace() {
    let mut fixture = Fixture::new("rustcraft_java:piston");
    fixture.world.watch_java_bud(BASE);
    fixture.place(QC, "rustcraft_java:redstone_block");
    assert_eq!(
        fixture.world.inspect_java_bud(BASE).status,
        BudStatus::MissingUpdate
    );
    for _ in 0..256 {
        fixture.notify(JavaNotification::ShapeUpdate);
    }
    let diagnosis = fixture.world.inspect_java_bud(BASE);
    assert_eq!(diagnosis.status, BudStatus::HistoryInsufficient);
    assert_eq!(diagnosis.activation, Some(ActivationPower::Quasi(QC)));
    assert!(diagnosis.missing_baseline.is_none());
    assert!(diagnosis.missing_since.is_none());
}

#[test]
fn unavailable_sticky_piston_load_excludes_a_retraction_from_missing_update() {
    let mut fixture = Fixture::new("rustcraft_java:piston");
    let base = BlockPos::new(14, 11, 9);
    let quasi = BlockPos::new(14, 12, 8);
    let chunk = ChunkPos { x: 1, z: 0 };
    fixture.place(base, "rustcraft_java:sticky_piston");
    fixture.place(quasi, "rustcraft_java:redstone_block");
    fixture.request(Mutation::NotifyJavaBlock {
        target: base,
        from: Face::Down,
        kind: JavaNotification::NeighborChanged,
    });
    for _ in 0..3 {
        fixture.step();
    }
    fixture.world.watch_java_bud(base);
    assert_eq!(
        fixture.world.inspect_java_bud(base).status,
        BudStatus::Consistent
    );
    fixture.request(Mutation::SetChunkActive {
        position: chunk,
        active: false,
    });
    fixture.place(quasi, "rustcraft:empty");
    let diagnosis = fixture.world.inspect_java_bud(base);
    assert_eq!(diagnosis.activation, Some(ActivationPower::Unpowered));
    assert_eq!(diagnosis.status, BudStatus::Unavailable);
    assert_eq!(
        diagnosis.obstruction,
        Some(PistonObstruction::InactiveChunk(BlockPos::new(16, 11, 9)))
    );
    assert!(diagnosis.missing_since.is_none());
}
