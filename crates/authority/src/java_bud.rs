//! Read-only QC diagnostics and an opt-in trace for one block position.

use std::collections::{BTreeMap, VecDeque};

use rustcraft_world::BlockPos;

use crate::java_block_ticks::ScheduledBlockTick;
use crate::java_ejector::EjectorKind;
use crate::java_signal::{ActivationPower, DEVICE_FACES, Face};
use crate::{
    JavaBlockTickKind, PistonEvent, PistonKind, PistonTrace, PistonTraceStage, Server,
    redstone::Device,
};

const TRACE_CAPACITY: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudComponent {
    Piston,
    StickyPiston,
    Dispenser,
    Dropper,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PistonObstruction {
    WorldBoundary(BlockPos),
    InactiveChunk(BlockPos),
    Player(BlockPos),
    PushLimit(BlockPos),
    Immovable {
        position: BlockPos,
        state: Option<&'static str>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudTarget {
    pub component: BudComponent,
    pub facing: Face,
    pub active: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudStatus {
    Unsupported,
    Unavailable,
    Consistent,
    Pending,
    Moving,
    Blocked,
    HistoryInsufficient,
    MissingUpdate,
}

impl BudStatus {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Unavailable => "unavailable",
            Self::Consistent => "consistent",
            Self::Pending => "pending",
            Self::Moving => "moving",
            Self::Blocked => "blocked",
            Self::HistoryInsufficient => "history_insufficient",
            Self::MissingUpdate => "missing_update",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BudPhase {
    #[default]
    Boundary,
    Commit,
    ScheduledBlockTick,
    BlockEvent,
    Player,
    ItemMotion,
    PistonMotion,
    ChunkView,
    Publish,
}

impl From<crate::schedule::Phase> for BudPhase {
    fn from(phase: crate::schedule::Phase) -> Self {
        use crate::schedule::Phase;
        match phase {
            Phase::Commit => Self::Commit,
            Phase::JavaScheduledBlocks => Self::ScheduledBlockTick,
            Phase::JavaBlockEvents => Self::BlockEvent,
            Phase::SimulatePlayer => Self::Player,
            Phase::JavaItemMotion => Self::ItemMotion,
            Phase::JavaPistonMotion => Self::PistonMotion,
            Phase::RefreshChunkView => Self::ChunkView,
            Phase::Publish => Self::Publish,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudStamp {
    pub gt: u64,
    pub phase: BudPhase,
    pub sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EjectorOutcome {
    Canceled,
    DeferredChunk,
    DeferredRandom,
    Empty,
    TransferBlocked,
    Transferred,
    Ejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudRecordKind {
    Observed {
        state: Option<&'static str>,
        target: Option<BudTarget>,
        activation: Option<ActivationPower>,
        status: BudStatus,
    },
    Edit {
        before: &'static str,
        after: &'static str,
    },
    Shape {
        from: Face,
    },
    Check {
        source: Option<BlockPos>,
        activation: ActivationPower,
    },
    BlockEvent(PistonTrace),
    Scheduled {
        tick: ScheduledBlockTick<JavaBlockTickKind>,
        inserted: bool,
    },
    Callback {
        tick: ScheduledBlockTick<JavaBlockTickKind>,
        outcome: EjectorOutcome,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudRecord {
    pub stamp: BudStamp,
    pub parent_sequence: Option<u64>,
    pub kind: BudRecordKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudMotion {
    pub event: PistonEvent,
    pub position: BlockPos,
    pub remaining: u8,
    pub previous_remaining: u8,
    pub awaiting_activation: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BudDiagnosis {
    pub gt: u64,
    pub position: BlockPos,
    pub state: Option<&'static str>,
    pub target: Option<BudTarget>,
    pub activation: Option<ActivationPower>,
    pub status: BudStatus,
    pub obstruction: Option<PistonObstruction>,
    pub piston_events: Vec<PistonEvent>,
    pub scheduled_ticks: Vec<ScheduledBlockTick<JavaBlockTickKind>>,
    pub motion: Vec<BudMotion>,
    pub last_check: Option<BudRecord>,
    pub last_block_event: Option<BudRecord>,
    pub last_scheduled_event: Option<BudRecord>,
    pub missing_baseline: Option<BudStamp>,
    pub missing_since: Option<BudStamp>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Observation {
    state: Option<&'static str>,
    target: Option<BudTarget>,
    activation: Option<ActivationPower>,
    status: BudStatus,
}

impl From<&BudDiagnosis> for Observation {
    fn from(diagnosis: &BudDiagnosis) -> Self {
        Self {
            state: diagnosis.state,
            target: diagnosis.target,
            activation: diagnosis.activation,
            status: diagnosis.status,
        }
    }
}

pub(crate) struct BudWatch {
    pub position: BlockPos,
    pub phase: BudPhase,
    records: VecDeque<BudRecord>,
    sequence: u64,
    observed: Option<Observation>,
    observed_check: u64,
    last_check: u64,
    history_after: u64,
    missing: Option<(BudStamp, BudStamp)>,
    queued_pistons: BTreeMap<PistonEvent, u64>,
}

impl BudWatch {
    fn new(position: BlockPos) -> Self {
        Self {
            position,
            phase: BudPhase::Boundary,
            records: VecDeque::new(),
            sequence: 0,
            observed: None,
            observed_check: 0,
            last_check: 0,
            history_after: 0,
            missing: None,
            queued_pistons: BTreeMap::new(),
        }
    }

    fn push(&mut self, gt: u64, kind: BudRecordKind) -> BudStamp {
        self.sequence += 1;
        let stamp = BudStamp {
            gt,
            phase: self.phase,
            sequence: self.sequence,
        };
        let parent_sequence = match kind {
            BudRecordKind::Scheduled { .. }
            | BudRecordKind::BlockEvent(PistonTrace { stage: PistonTraceStage::Queued | PistonTraceStage::Duplicate, .. }) => {
                self.records.back().filter(|record| matches!(record.kind, BudRecordKind::Check { .. }))
                    .map(|record| record.stamp.sequence)
            }
            BudRecordKind::Callback { tick, .. } => self.last(|kind| matches!(
                kind,
                BudRecordKind::Scheduled { tick: scheduled, inserted: true } if scheduled == tick
            )).map(|record| record.stamp.sequence),
            BudRecordKind::BlockEvent(PistonTrace {
                event, stage: PistonTraceStage::Deferred, ..
            }) => self.queued_pistons.get(&event).copied(),
            BudRecordKind::BlockEvent(PistonTrace {
                event,
                stage: PistonTraceStage::Started | PistonTraceStage::Canceled | PistonTraceStage::Blocked,
                ..
            }) => self.queued_pistons.remove(&event),
            _ => None,
        };
        if self.records.len() == TRACE_CAPACITY {
            self.records.pop_front();
        }
        self.records.push_back(BudRecord {
            stamp,
            parent_sequence,
            kind,
        });
        if matches!(kind, BudRecordKind::Check { .. }) {
            self.last_check = stamp.sequence;
        }
        if let BudRecordKind::BlockEvent(PistonTrace {
            event,
            stage: PistonTraceStage::Queued,
            ..
        }) = kind
        {
            self.queued_pistons.insert(event, stamp.sequence);
        }
        stamp
    }

    fn reset_evidence(&mut self) {
        self.observed = None;
        self.missing = None;
        self.history_after = self.sequence;
        self.queued_pistons.clear();
    }

    fn observe(&mut self, gt: u64, observation: Observation) {
        if matches!(
            observation.status,
            BudStatus::Unavailable | BudStatus::Unsupported
        ) {
            self.missing = None;
            if self.observed.is_some_and(|old| old.target.is_some()) {
                self.reset_evidence();
            }
        }
        let previous = self.observed;
        let no_check = self.last_check == self.observed_check;
        if !no_check
            || observation.status != BudStatus::HistoryInsufficient
            || previous.is_none_or(|old| old.target != observation.target)
        {
            self.missing = None;
        }
        if previous != Some(observation) {
            let missing = no_check
                && observation.status == BudStatus::HistoryInsufficient
                && previous.is_some_and(|old| {
                    old.status == BudStatus::Consistent
                        && old.target == observation.target
                        && old.activation != observation.activation
                });
            let baseline = if missing {
                self.last(|kind| {
                    matches!(
                        kind,
                        BudRecordKind::Observed {
                            status: BudStatus::Consistent,
                            ..
                        }
                    )
                })
                .filter(|record| {
                    self.records.len() < TRACE_CAPACITY
                        || self
                            .records
                            .front()
                            .is_some_and(|first| first.stamp != record.stamp)
                })
                .map(|record| record.stamp)
            } else {
                None
            };
            let stamp = self.push(
                gt,
                BudRecordKind::Observed {
                    state: observation.state,
                    target: observation.target,
                    activation: observation.activation,
                    status: if baseline.is_some() {
                        BudStatus::MissingUpdate
                    } else {
                        observation.status
                    },
                },
            );
            if let Some(baseline) = baseline {
                self.missing = Some((baseline, stamp));
            }
        }
        self.observed = Some(observation);
        self.observed_check = self.last_check;
    }

    fn last(&self, filter: impl Fn(BudRecordKind) -> bool) -> Option<BudRecord> {
        self.records
            .iter()
            .rev()
            .take_while(|record| record.stamp.sequence > self.history_after)
            .find(|record| filter(record.kind))
            .copied()
    }
}

fn piston_component(kind: PistonKind) -> BudComponent {
    match kind {
        PistonKind::Normal => BudComponent::Piston,
        PistonKind::Sticky => BudComponent::StickyPiston,
    }
}

fn target(device: Device) -> Option<BudTarget> {
    let (component, facing, active) = match device {
        Device::Piston(facing, active, kind) => (piston_component(kind), facing, Some(active)),
        Device::Moving(facing, kind) => (piston_component(kind), facing, None),
        Device::Ejector(ejector) => (
            match ejector.kind {
                EjectorKind::Dispenser => BudComponent::Dispenser,
                EjectorKind::Dropper => BudComponent::Dropper,
            },
            ejector.facing,
            Some(ejector.triggered),
        ),
        _ => return None,
    };
    Some(BudTarget {
        component,
        facing: DEVICE_FACES[facing],
        active,
    })
}

impl Server {
    /// Start observing one position. History is local to this running Server.
    pub fn watch_java_bud(&mut self, position: BlockPos) {
        if let Some(logic) = &mut self.java_redstone {
            logic.bud_watch = Some(BudWatch::new(position));
            self.observe_java_bud();
        }
    }

    pub fn java_bud_trace(&self) -> impl Iterator<Item = &BudRecord> {
        self.java_redstone
            .iter()
            .filter_map(|logic| logic.bud_watch.as_ref())
            .flat_map(|watch| &watch.records)
    }

    pub fn inspect_java_bud(&self, position: BlockPos) -> BudDiagnosis {
        let mut diagnosis = self.bud_state(position);
        if let Some(watch) = self
            .java_redstone
            .as_ref()
            .and_then(|logic| logic.bud_watch.as_ref())
            && watch.position == position
        {
            diagnosis.last_check = watch.last(|kind| matches!(kind, BudRecordKind::Check { .. }));
            diagnosis.last_block_event =
                watch.last(|kind| matches!(kind, BudRecordKind::BlockEvent(_)));
            diagnosis.last_scheduled_event = watch.last(|kind| {
                matches!(
                    kind,
                    BudRecordKind::Scheduled { .. } | BudRecordKind::Callback { .. }
                )
            });
            if diagnosis.status == BudStatus::HistoryInsufficient
                && watch.observed == Some(Observation::from(&diagnosis))
                && watch.observed_check == watch.last_check
                && let Some((baseline, since)) = watch.missing
                && watch
                    .records
                    .front()
                    .is_some_and(|first| first.stamp.sequence <= baseline.sequence)
            {
                diagnosis.status = BudStatus::MissingUpdate;
                diagnosis.missing_baseline = Some(baseline);
                diagnosis.missing_since = Some(since);
            }
        }
        diagnosis
    }

    pub(crate) fn observe_java_bud(&mut self) {
        let Some(position) = self
            .java_redstone
            .as_ref()
            .and_then(|logic| logic.bud_watch.as_ref())
            .map(|watch| watch.position)
        else {
            return;
        };
        let observation = Observation::from(&self.bud_state(position));
        self.java_redstone
            .as_mut()
            .unwrap()
            .bud_watch
            .as_mut()
            .unwrap()
            .observe(self.tick, observation);
    }

    pub(crate) fn record_bud(&mut self, position: BlockPos, kind: BudRecordKind) {
        if let Some(watch) = self
            .java_redstone
            .as_mut()
            .and_then(|logic| logic.bud_watch.as_mut())
            && watch.position == position
        {
            watch.push(self.tick, kind);
        }
    }

    pub(crate) fn record_bud_edit(&mut self, edit: &crate::java_redstone::Edit) {
        if self
            .java_redstone
            .as_ref()
            .and_then(|logic| logic.bud_watch.as_ref())
            .is_none_or(|watch| watch.position != edit.position)
        {
            return;
        }
        let key = |block| {
            self.device(block)
                .and_then(target)
                .map(|target| (target.component, target.facing))
        };
        let before = self.registry.block(edit.before).unwrap().name;
        let after = self.registry.block(edit.after).unwrap().name;
        if key(edit.before) != key(edit.after) {
            self.java_redstone
                .as_mut()
                .unwrap()
                .bud_watch
                .as_mut()
                .unwrap()
                .reset_evidence();
        }
        self.record_bud(edit.position, BudRecordKind::Edit { before, after });
    }

    fn bud_state(&self, position: BlockPos) -> BudDiagnosis {
        let block = self.dimension.voxel(position).map(|(block, _)| block);
        let mut diagnosis = BudDiagnosis {
            gt: self.tick,
            position,
            state: block
                .and_then(|id| self.registry.block(id))
                .map(|block| block.name),
            target: None,
            activation: None,
            status: BudStatus::Unsupported,
            obstruction: None,
            piston_events: Vec::new(),
            scheduled_ticks: Vec::new(),
            motion: Vec::new(),
            last_check: None,
            last_block_event: None,
            last_scheduled_event: None,
            missing_baseline: None,
            missing_since: None,
        };
        let Some(logic) = &self.java_redstone else {
            return diagnosis;
        };
        diagnosis.motion = logic
            .motions
            .iter()
            .filter(|motion| motion.event.base == position)
            .map(|motion| BudMotion {
                event: motion.event,
                position: motion.position,
                remaining: motion.remaining,
                previous_remaining: motion.previous_remaining,
                awaiting_activation: motion.awaiting_activation,
            })
            .collect();
        let Some(block) = block else {
            diagnosis.status = BudStatus::Unavailable;
            diagnosis.piston_events = logic
                .pending()
                .filter(|event| event.base == position)
                .collect();
            diagnosis.scheduled_ticks = self
                .pending_java_block_ticks()
                .filter(|tick| {
                    tick.position == position
                        && matches!(
                            tick.kind,
                            JavaBlockTickKind::Dispenser | JavaBlockTickKind::Dropper
                        )
                })
                .collect();
            return diagnosis;
        };
        let Some(device) = self.device(block) else {
            return diagnosis;
        };
        let mut current = target(device);
        if matches!(device, Device::Moving(..)) {
            current = current.filter(|_| !diagnosis.motion.is_empty());
        }
        let Some(current) = current else {
            return diagnosis;
        };
        diagnosis.target = Some(current);
        diagnosis.activation = match device {
            Device::Piston(..) | Device::Moving(..) => {
                Some(self.piston_activation_at(position, current.facing))
            }
            Device::Ejector(_) => Some(self.java_ejector_activation(position)),
            _ => None,
        };
        match device {
            Device::Ejector(ejector) => {
                diagnosis.scheduled_ticks = self
                    .pending_java_block_ticks()
                    .filter(|tick| {
                        tick.position == position && tick.kind == ejector.kind.callback()
                    })
                    .collect();
            }
            _ => {
                diagnosis.piston_events = logic
                    .pending()
                    .filter(|event| {
                        event.base == position
                            && piston_component(event.kind) == current.component
                            && DEVICE_FACES[usize::from(event.facing)] == current.facing
                    })
                    .collect();
            }
        }
        let powered = diagnosis.activation.and_then(ActivationPower::powered);
        if let Device::Piston(facing, extended, kind) = device
            && powered == Some(!extended)
            && diagnosis.motion.is_empty()
        {
            diagnosis.obstruction = match current.facing.offset(position) {
                Some(head) if extended => self
                    .piston_retraction_load(kind, current.facing, head)
                    .err(),
                Some(head) => self.piston_push_plan(head, facing).err(),
                None => Some(PistonObstruction::WorldBoundary(position)),
            };
        }
        diagnosis.status = if !diagnosis.motion.is_empty() {
            BudStatus::Moving
        } else if !diagnosis.piston_events.is_empty() || !diagnosis.scheduled_ticks.is_empty() {
            BudStatus::Pending
        } else if matches!(
            diagnosis.obstruction,
            Some(PistonObstruction::InactiveChunk(_))
        ) || powered.is_none()
        {
            BudStatus::Unavailable
        } else if diagnosis.obstruction.is_some() {
            BudStatus::Blocked
        } else if powered == current.active {
            BudStatus::Consistent
        } else {
            BudStatus::HistoryInsufficient
        };
        diagnosis
    }
}
