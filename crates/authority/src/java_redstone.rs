use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rustcraft_world::{BlockPos, BlockStateId};

use crate::java_block_ticks::{BlockTickPriority, JavaBlockTicks, ScheduledBlockTick};
use crate::java_bud::{BudRecordKind, BudWatch, PistonObstruction};
use crate::java_signal::{
    ActivationPower, DEVICE_FACES, Emitter, Face, PistonPower, PowerContext, PowerOutput, Signal,
    SignalCell, piston_power, power_output,
};
use crate::java_wire::wire_power_notice_centers;
use crate::{Effect, Rejection, Server, redstone::Device};

const HORIZONTAL_FACES: [Face; 4] = [
    DEVICE_FACES[0],
    DEVICE_FACES[1],
    DEVICE_FACES[2],
    DEVICE_FACES[3],
];

fn neighbor_faces_except(excluded: Face) -> impl Iterator<Item = Face> {
    Face::ALL.into_iter().filter(move |face| *face != excluded)
}
pub(crate) const MOTION_TICKS: u8 = 3;
const EDIT_LIMIT: usize = 4096;
const MAX_SCHEDULED_BLOCK_TICKS: usize = 65_536;

pub(crate) struct Edit {
    pub position: BlockPos,
    pub before: BlockStateId,
    pub after: BlockStateId,
    pub notifications: EditNotification,
}

#[derive(Clone, Copy)]
pub(crate) enum EditNotification {
    All,
    HostSetBlock,
    HostNeighbors,
    ShapesOnly,
    WireToggle(u8),
    WirePower,
    DirectionalOutput(Face),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JavaNotification {
    NeighborChanged,
    ShapeUpdate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum JavaBlockTickKind {
    Lamp,
    Repeater,
    Comparator,
    Observer,
    RedstoneTorch,
    RedstoneWallTorch,
    StoneButton,
    OakButton,
    StonePressurePlate,
    OakPressurePlate,
    LightPressurePlate,
    HeavyPressurePlate,
    Dispenser,
    Dropper,
}

impl JavaBlockTickKind {
    pub fn allows_priority(self, priority: BlockTickPriority) -> bool {
        match self {
            Self::Repeater => priority <= BlockTickPriority::Normal,
            Self::Comparator => matches!(
                priority,
                BlockTickPriority::High | BlockTickPriority::Normal
            ),
            _ => priority == BlockTickPriority::Normal,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PistonKind {
    Normal,
    Sticky,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PistonAction {
    Extend,
    Retract,
    Drop,
}

impl PistonAction {
    pub const fn java_event_id(self) -> u8 {
        match self {
            Self::Extend => 0,
            Self::Retract => 1,
            Self::Drop => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PistonEvent {
    pub base: BlockPos,
    pub facing: u8,
    pub action: PistonAction,
    pub kind: PistonKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PistonTraceStage {
    Queued,
    Duplicate,
    Deferred,
    Canceled,
    Blocked,
    Started,
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PistonTrace {
    pub gt: u64,
    pub event: PistonEvent,
    pub stage: PistonTraceStage,
    pub source: Option<BlockPos>,
    pub moving_position: Option<BlockPos>,
    pub power: Option<PistonPower>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Motion {
    pub event: PistonEvent,
    pub position: BlockPos,
    pub result: BlockStateId,
    pub remaining: u8,
    pub previous_remaining: u8,
    pub awaiting_activation: bool,
}

impl Motion {
    pub(crate) fn marker(&self) -> Device {
        let event = self.event;
        let facing = usize::from(event.facing);
        let source = match event.action {
            PistonAction::Extend => DEVICE_FACES[facing].offset(event.base),
            PistonAction::Retract | PistonAction::Drop => Some(event.base),
        };
        let kind = if source == Some(self.position) {
            event.kind
        } else {
            PistonKind::Normal
        };
        Device::Moving(facing, kind)
    }
}

#[derive(Default)]
pub(crate) struct JavaRedstone {
    events: VecDeque<PistonEvent>,
    identities: BTreeSet<PistonEvent>,
    pub motions: VecDeque<Motion>,
    pub trace: Vec<PistonTrace>,
    pub bud_watch: Option<BudWatch>,
    pub block_ticks: JavaBlockTicks<JavaBlockTickKind>,
    pub draining_block_ticks: BTreeSet<(BlockPos, JavaBlockTickKind)>,
    pub next_block_tick_order: u64,
    pub(crate) torch_toggles: crate::java_torch::TorchToggles,
    pub(crate) comparator_outputs: BTreeMap<BlockPos, Signal>,
    pub(crate) ejector_inventories: BTreeMap<BlockPos, [Option<crate::ItemStack>; 9]>,
}

impl JavaRedstone {
    pub(crate) fn enqueue(&mut self, event: PistonEvent) -> bool {
        if !self.identities.insert(event) {
            return false;
        }
        self.events.push_back(event);
        true
    }

    fn pop(&mut self) -> Option<PistonEvent> {
        let event = self.events.pop_front()?;
        self.identities.remove(&event);
        Some(event)
    }

    pub(crate) fn pending(&self) -> impl Iterator<Item = PistonEvent> + '_ {
        self.events.iter().copied()
    }

    pub(crate) fn is_moving(&self, position: BlockPos) -> bool {
        self.motions
            .iter()
            .any(|motion| motion.position == position || motion.event.base == position)
    }
}

impl Server {
    pub(crate) fn update_java_lamp(&mut self, position: BlockPos, lit: bool) {
        match (self.java_neighbor_signal(position), lit) {
            (Some(true), false) => {
                self.write_java_device_state(
                    position,
                    Device::Lamp(true),
                    EditNotification::ShapesOnly,
                );
            }
            (Some(false), true) => {
                self.schedule_java_block_tick(
                    position,
                    JavaBlockTickKind::Lamp,
                    BlockTickPriority::Normal,
                    crate::time::java_redstone_delay(2),
                );
            }
            _ => {}
        }
    }

    pub(crate) fn schedule_java_block_tick(
        &mut self,
        position: BlockPos,
        kind: JavaBlockTickKind,
        priority: BlockTickPriority,
        delay_gt: u64,
    ) {
        let logic = self.java_redstone.as_mut().expect("Java redstone rule");
        let due_gt = self
            .tick
            .checked_add(delay_gt)
            .expect("Java block tick time exhausted");
        let sub_tick_order = logic.next_block_tick_order;
        logic.next_block_tick_order = sub_tick_order
            .checked_add(1)
            .expect("Java block tick sequence exhausted");
        let tick = ScheduledBlockTick {
            due_gt,
            priority,
            sub_tick_order,
            position,
            kind,
        };
        let inserted = logic.block_ticks.schedule(tick);
        if matches!(
            kind,
            JavaBlockTickKind::Dispenser | JavaBlockTickKind::Dropper
        ) {
            self.record_bud(position, BudRecordKind::Scheduled { tick, inserted });
        }
    }

    pub(crate) fn java_neighbor_signal(&self, position: BlockPos) -> Option<bool> {
        self.java_neighbor_activation(position).powered()
    }

    pub(crate) fn java_neighbor_activation(&self, position: BlockPos) -> ActivationPower {
        let mut incomplete = false;
        for face in Face::ALL {
            let Some(source) = face.offset(position) else {
                continue;
            };
            let power = power_output(source, face.opposite(), PowerContext::Normal, |pos| {
                self.java_signal_cell(pos)
            });
            if let Some(output) = power
                && (output.normal != Signal::OFF
                    || output.conducted.is_some_and(|signal| signal != Signal::OFF))
            {
                return ActivationPower::Direct(source);
            }
            incomplete |= power.is_none_or(|output| output.conducted.is_none());
        }
        if incomplete {
            ActivationPower::Unavailable
        } else {
            ActivationPower::Unpowered
        }
    }

    pub fn pending_java_block_ticks(
        &self,
    ) -> impl Iterator<Item = ScheduledBlockTick<JavaBlockTickKind>> + '_ {
        self.java_redstone
            .iter()
            .flat_map(|logic| logic.block_ticks.pending())
    }

    pub(crate) fn java_wire_external_input(&self, position: BlockPos) -> Option<(BlockPos, u8)> {
        let mut strongest = None;
        for face in Face::ALL {
            let Some(source) = face.offset(position) else {
                continue;
            };
            let Some(output) = power_output(
                source,
                face.opposite(),
                PowerContext::WireExternalInput,
                |cell| self.java_signal_cell(cell),
            ) else {
                continue;
            };
            let power = output
                .normal
                .max(output.conducted.unwrap_or(Signal::OFF))
                .get();
            if power > strongest.map_or(0, |(_, strength)| strength) {
                strongest = Some((source, power));
            }
            if power == Signal::MAX.get() {
                break;
            }
        }
        strongest
    }

    pub(crate) fn run_java_scheduled_blocks(&mut self) {
        let logic = self.java_redstone.as_mut().unwrap();
        let due = logic
            .block_ticks
            .collect_due(self.tick, MAX_SCHEDULED_BLOCK_TICKS, |chunk| {
                self.dimension.is_active(chunk)
            });
        logic
            .draining_block_ticks
            .extend(due.iter().map(|tick| (tick.position, tick.kind)));
        for tick in due {
            self.java_redstone
                .as_mut()
                .unwrap()
                .draining_block_ticks
                .remove(&(tick.position, tick.kind));
            match tick.kind {
                JavaBlockTickKind::Lamp => {
                    if self.read_device(tick.position) == Some(Device::Lamp(true)) {
                        match self.java_neighbor_signal(tick.position) {
                            Some(false) => {
                                self.write_java_device_state(
                                    tick.position,
                                    Device::Lamp(false),
                                    EditNotification::ShapesOnly,
                                );
                            }
                            Some(true) => {}
                            None => {
                                self.java_redstone
                                    .as_mut()
                                    .unwrap()
                                    .block_ticks
                                    .schedule(tick);
                            }
                        }
                    }
                }
                JavaBlockTickKind::Repeater => self.run_java_repeater(tick),
                JavaBlockTickKind::Dispenser | JavaBlockTickKind::Dropper => {
                    let outcome = self.run_java_ejector(tick);
                    self.record_bud(tick.position, BudRecordKind::Callback { tick, outcome });
                }
                JavaBlockTickKind::Comparator => self.run_java_comparator(tick),
                JavaBlockTickKind::Observer => self.run_java_observer(tick.position),
                JavaBlockTickKind::RedstoneTorch | JavaBlockTickKind::RedstoneWallTorch => {
                    self.run_java_torch(tick);
                }
                JavaBlockTickKind::StoneButton
                | JavaBlockTickKind::OakButton
                | JavaBlockTickKind::StonePressurePlate
                | JavaBlockTickKind::OakPressurePlate
                | JavaBlockTickKind::LightPressurePlate
                | JavaBlockTickKind::HeavyPressurePlate => {
                    self.run_java_control(tick);
                }
            }
            self.dispatch_java_edits();
        }
    }

    pub fn pending_piston_events(&self) -> impl Iterator<Item = PistonEvent> + '_ {
        self.java_redstone.iter().flat_map(JavaRedstone::pending)
    }

    pub(crate) fn write_java_device_state(
        &mut self,
        position: BlockPos,
        device: Device,
        notifications: EditNotification,
    ) {
        if let Some((_, revision)) = self.dimension.voxel(position) {
            let _ = self.dimension.edit_block_with_notifications(
                position,
                device.state(&self.registry),
                revision,
                notifications,
            );
        }
    }

    pub(crate) fn dispatch_java_edits(&mut self) {
        for _ in 0..EDIT_LIMIT {
            let Some(edit) = self.take_java_edit(0) else {
                return;
            };
            self.process_java_edit(edit);
        }
    }

    fn java_edit_count(&self) -> usize {
        self.dimension
            .java_redstone_edits
            .as_ref()
            .map_or(0, VecDeque::len)
    }

    fn take_java_edit(&mut self, index: usize) -> Option<Edit> {
        self.dimension
            .java_redstone_edits
            .as_mut()
            .and_then(|edits| edits.remove(index))
    }

    fn dispatch_java_edits_from(&mut self, mark: usize) {
        while self.java_edit_count() > mark {
            let Some(edit) = self.take_java_edit(mark) else {
                return;
            };
            self.process_java_edit(edit);
        }
    }

    fn process_java_edit(&mut self, edit: Edit) {
        if !matches!(edit.notifications, EditNotification::HostNeighbors) {
            self.record_bud_edit(&edit);
        }
        match edit.notifications {
            EditNotification::All | EditNotification::HostSetBlock => {}
            EditNotification::HostNeighbors => {
                self.notify_java_edit_neighbors(&edit);
                return;
            }
            EditNotification::ShapesOnly => {
                self.notify_java_shapes(edit.position);
                return;
            }
            EditNotification::WireToggle(changed_ports) => {
                self.notify_java_shapes(edit.position);
                self.notify_java_wire_toggle(edit.position, changed_ports);
                return;
            }
            EditNotification::WirePower => {
                self.notify_java_shapes(edit.position);
                for source in wire_power_notice_centers(edit.position)
                    .into_iter()
                    .flatten()
                {
                    self.notify_java_neighbors_depth_first(source);
                }
                return;
            }
            EditNotification::DirectionalOutput(output) => {
                self.notify_java_shapes(edit.position);
                self.notify_java_output(edit.position, output);
                return;
            }
        }
        let before = self.device(edit.before);
        let after = self.device(edit.after);
        if matches!(before, Some(Device::Dust(..))) && !matches!(after, Some(Device::Dust(..))) {
            for face in Face::JAVA_ORDER {
                if let Some(source) = face.offset(edit.position) {
                    self.notify_java_neighbors(source);
                }
            }
        }
        if let Some(Device::Ejector(previous)) = before
            && !matches!(after, Some(Device::Ejector(current)) if current.kind == previous.kind)
        {
            self.remove_java_ejector_inventory(edit.position);
        }
        if matches!(before, Some(Device::Comparator(_)))
            && !matches!(after, Some(Device::Comparator(_)))
        {
            self.java_redstone
                .as_mut()
                .unwrap()
                .comparator_outputs
                .remove(&edit.position);
        }
        if let Some(Device::Piston(_, _, kind)) = after {
            if !matches!(before, Some(Device::Piston(_, _, previous)) if previous == kind) {
                self.check_piston(edit.position, None);
            }
        } else {
            if let Some(support) = after.and_then(Device::attachment_face) {
                self.java_shape_changed(edit.position, support);
            }
            if !matches!(
                after,
                Some(
                    Device::Repeater(_)
                        | Device::Comparator(_)
                        | Device::Lamp(_)
                        | Device::Ejector(_)
                )
            ) {
                self.update_device(edit.position, None);
            }
        }
        if matches!(edit.notifications, EditNotification::HostSetBlock) {
            // A setblock-style write sends shapes before explicitly updating neighbors.
            self.notify_java_shapes(edit.position);
            self.dimension
                .java_redstone_edits
                .as_mut()
                .unwrap()
                .push_back(Edit {
                    notifications: EditNotification::HostNeighbors,
                    ..edit
                });
        } else {
            self.notify_java_edit_neighbors(&edit);
        }
    }

    fn notify_java_edit_neighbors(&mut self, edit: &Edit) {
        self.notify_java_neighbors(edit.position);
        if matches!(edit.notifications, EditNotification::All) {
            self.notify_java_shapes(edit.position);
        }
        self.notify_java_control_and_slope_neighbors(edit);
    }

    fn notify_java_neighbors(&mut self, source: BlockPos) {
        for face in Face::ALL {
            if let Some(target) = face.offset(source) {
                self.java_neighbor_changed(target, source);
            }
        }
    }

    fn notify_java_neighbors_depth_first(&mut self, source: BlockPos) {
        for face in Face::ALL {
            if let Some(target) = face.offset(source) {
                let mark = self.java_edit_count();
                self.java_neighbor_changed(target, source);
                self.dispatch_java_edits_from(mark);
            }
        }
    }

    fn notify_java_control_and_slope_neighbors(&mut self, edit: &Edit) {
        let before = self.device(edit.before);
        let after = self.device(edit.after);
        let changes_slope = matches!(before, Some(Device::Dust(..)))
            != matches!(after, Some(Device::Dust(..)))
            || self.java_conductor(edit.before) != self.java_conductor(edit.after)
            || self.dimension.is_passable(edit.before) != self.dimension.is_passable(edit.after);
        let mut old_control = before.and_then(Device::control_support_signal);
        let mut new_control = after.and_then(Device::control_support_signal);
        if matches!(edit.notifications, EditNotification::HostNeighbors)
            && matches!(after, Some(Device::Lever(_)))
        {
            // A state write does not run the lever's player-use notification.
            new_control = None;
            if matches!(before, Some(Device::Lever(_))) {
                old_control = None;
            }
        }
        if old_control != new_control {
            if let Some((old_support, _)) = old_control {
                self.notify_java_output(edit.position, old_support);
            }
            if let Some((new_support, _)) = new_control
                && old_control.is_none_or(|(old_support, _)| old_support != new_support)
            {
                self.notify_java_output(edit.position, new_support);
            }
        }
        for face in HORIZONTAL_FACES {
            let Some(side) = face.offset(edit.position) else {
                continue;
            };
            for vertical in [Face::Down, Face::Up] {
                if let Some(target) = vertical.offset(side)
                    && let Some(Device::Dust(power, shape)) = self.read_device(target)
                {
                    if changes_slope {
                        self.update_java_wire_shape(target, face.opposite(), power, shape);
                    }
                    self.update_device(target, Some(edit.position));
                }
            }
        }
    }

    fn notify_java_wire_toggle(&mut self, position: BlockPos, changed_ports: u8) {
        for (direction, side) in HORIZONTAL_FACES.into_iter().enumerate() {
            if changed_ports & (1 << direction) == 0 {
                continue;
            }
            if let Some(front) = side.offset(position)
                && self
                    .dimension
                    .voxel(front)
                    .is_some_and(|(block, _)| self.java_conductor(block))
            {
                for face in neighbor_faces_except(side.opposite()) {
                    if let Some(target) = face.offset(front) {
                        self.java_neighbor_changed(target, front);
                    }
                }
            }
        }
    }

    fn notify_java_shapes(&mut self, position: BlockPos) {
        for face in Face::ALL {
            if let Some(target) = face.offset(position) {
                self.java_shape_changed(target, face.opposite());
            }
        }
    }

    pub(crate) fn notify_java_output(&mut self, position: BlockPos, output: Face) {
        let Some(front) = output.offset(position) else {
            return;
        };
        self.java_neighbor_changed(front, position);
        for face in neighbor_faces_except(output.opposite()) {
            if let Some(target) = face.offset(front) {
                self.java_neighbor_changed(target, front);
            }
        }
    }

    pub(crate) fn notify_java_block(
        &mut self,
        target: BlockPos,
        from: Face,
        kind: JavaNotification,
    ) -> Result<Effect, Rejection> {
        if self.java_redstone.is_none() {
            return Err(Rejection::MissingCapability);
        }
        let source = from.offset(target).ok_or(Rejection::OutOfWorld)?;
        if self.dimension.voxel(target).is_none() || self.dimension.voxel(source).is_none() {
            return Err(Rejection::InactiveChunk);
        }
        match kind {
            JavaNotification::NeighborChanged => self.java_neighbor_changed(target, source),
            JavaNotification::ShapeUpdate => self.java_shape_changed(target, from),
        }
        Ok(Effect::Unchanged)
    }

    pub(crate) fn notify_java_neighbors_except(
        &mut self,
        source: BlockPos,
        excluded: Face,
    ) -> Result<Effect, Rejection> {
        if self.java_redstone.is_none() {
            return Err(Rejection::MissingCapability);
        }
        if self.dimension.voxel(source).is_none() {
            return Err(Rejection::InactiveChunk);
        }
        for face in neighbor_faces_except(excluded) {
            if let Some(target) = face.offset(source)
                && self.dimension.voxel(target).is_some()
            {
                self.java_neighbor_changed(target, source);
            }
        }
        Ok(Effect::Unchanged)
    }

    fn java_shape_changed(&mut self, target: BlockPos, from: Face) {
        self.record_bud(target, BudRecordKind::Shape { from });
        if let Some(device) = self.read_device(target) {
            if device.attachment_face() == Some(from) && !self.has_redstone_attachment(target, from)
            {
                self.write_redstone(target, self.dimension.empty_state());
            } else if let Device::Dust(power, shape) = device {
                self.update_java_wire_shape(target, from, power, shape);
            } else if let Device::Repeater(repeater) = device {
                self.update_java_repeater_shape(target, repeater, from);
            } else if let Device::Observer(facing, false) = device
                && from == DEVICE_FACES[facing]
            {
                self.arm_java_observer(target);
            }
        }
    }

    fn java_neighbor_changed(&mut self, target: BlockPos, source: BlockPos) {
        if matches!(self.read_device(target), Some(Device::Piston(..))) {
            self.check_piston(target, Some(source));
        } else {
            self.update_device(target, Some(source));
        }
    }

    pub fn java_piston_power(&self, position: BlockPos) -> Option<PistonPower> {
        self.java_redstone.as_ref()?;
        let Device::Piston(facing, _, _) = self.read_device(position)? else {
            return None;
        };
        Some(self.piston_activation_at(position, DEVICE_FACES[facing]))
    }

    pub(crate) fn piston_activation_at(&self, position: BlockPos, facing: Face) -> ActivationPower {
        piston_power(position, facing, |pos| self.java_signal_cell(pos))
    }

    /// Observe the live power output toward a receiver in the Java reference preset.
    pub fn java_power_output(&self, source: BlockPos, toward: Face) -> Option<PowerOutput> {
        self.java_redstone.as_ref()?;
        power_output(source, toward, PowerContext::Normal, |position| {
            self.java_signal_cell(position)
        })
    }

    pub(crate) fn java_signal_cell(&self, position: BlockPos) -> Option<SignalCell> {
        if self
            .dimension
            .height
            .is_some_and(|range| !range.contains(position.y))
        {
            return Some(SignalCell::default());
        }
        let (id, _) = self.dimension.voxel(position)?;
        let conductor = self.java_conductor(id);
        let emitter = match self.device(id) {
            Some(Device::PowerBlock) => Emitter::RedstoneBlock,
            Some(Device::Lever(lever)) => Emitter::Lever {
                powered: lever.powered,
                support: lever.support(),
            },
            Some(Device::Button(button)) => Emitter::Button {
                powered: button.control.powered,
                support: button.control.support(),
            },
            Some(Device::PressurePlate(plate)) => Emitter::PressurePlate {
                signal: plate.power,
            },
            Some(Device::Repeater(repeater)) if repeater.powered => Emitter::Repeater {
                output: DEVICE_FACES[repeater.output],
            },
            Some(Device::Comparator(comparator)) if comparator.powered => Emitter::Comparator {
                output: DEVICE_FACES[comparator.output],
                signal: self.java_comparator_strength(position),
            },
            Some(Device::Observer(facing, true)) => Emitter::Observer {
                output: DEVICE_FACES[facing].opposite(),
            },
            Some(Device::Torch { support, lit: true }) => Emitter::Torch { support },
            Some(Device::Dust(power, shape)) => Emitter::Wire {
                power: Signal::new(power).expect("registered dust state"),
                ports: self.java_wire_output_ports(position, shape),
            },
            _ => Emitter::None,
        };
        Some(SignalCell { conductor, emitter })
    }

    pub(crate) fn java_conductor(&self, block: BlockStateId) -> bool {
        self.registry.block(block).is_some_and(|state| {
            matches!(
                state.name,
                "rustcraft_java:smooth_stone"
                    | "rustcraft_java:bedrock"
                    | "rustcraft_java:dirt"
                    | "rustcraft_java:grass_block"
                    | "rustcraft_java:redstone_lamp"
                    | "rustcraft_java:redstone_lamp_on"
            )
        })
    }

    fn check_piston(&mut self, base: BlockPos, source: Option<BlockPos>) {
        let Some(Device::Piston(facing, extended, kind)) = self.read_device(base) else {
            return;
        };
        let power = self
            .java_piston_power(base)
            .expect("piston in Java rule world");
        self.record_bud(
            base,
            BudRecordKind::Check {
                source,
                activation: power,
            },
        );
        let action = match (power, extended) {
            (PistonPower::Direct(_) | PistonPower::Quasi(_), false) => PistonAction::Extend,
            (PistonPower::Unpowered, true) => {
                let direction = DEVICE_FACES[facing];
                if direction
                    .offset(base)
                    .and_then(|head| direction.offset(head))
                    .is_some_and(|front| self.extending_motion(front, facing).is_some())
                {
                    PistonAction::Drop
                } else {
                    PistonAction::Retract
                }
            }
            _ => return,
        };
        if action == PistonAction::Extend
            && DEVICE_FACES[facing]
                .offset(base)
                .is_none_or(|head| self.piston_push_plan(head, facing).is_err())
        {
            return;
        }
        let event = PistonEvent {
            base,
            facing: facing as u8,
            action,
            kind,
        };
        let queued = self.java_redstone.as_mut().unwrap().enqueue(event);
        let stage = if queued {
            PistonTraceStage::Queued
        } else {
            PistonTraceStage::Duplicate
        };
        self.trace_piston(event, stage, source, None, Some(power));
    }

    pub(crate) fn run_java_block_events(&mut self) {
        let mut deferred = Vec::new();
        while let Some(event) = self.java_redstone.as_mut().unwrap().pop() {
            let stage = if !self.dimension.is_active(event.base.chunk()) {
                PistonTraceStage::Deferred
            } else {
                self.run_piston_event(event)
            };
            if stage == PistonTraceStage::Deferred {
                deferred.push(event);
            }
            if !matches!(stage, PistonTraceStage::Started) {
                self.trace_piston(event, stage, None, None, self.java_piston_power(event.base));
            }
            self.dispatch_java_edits();
        }
        for event in deferred {
            self.java_redstone.as_mut().unwrap().enqueue(event);
        }
    }

    fn run_piston_event(&mut self, event: PistonEvent) -> PistonTraceStage {
        use PistonTraceStage as Stage;
        let facing = usize::from(event.facing);
        let extend = event.action == PistonAction::Extend;
        if self.read_device(event.base) != Some(Device::Piston(facing, !extend, event.kind)) {
            return Stage::Canceled;
        }
        let power = self
            .java_piston_power(event.base)
            .expect("piston in Java rule world");
        if power == PistonPower::Unavailable {
            return Stage::Deferred;
        }
        if (power != PistonPower::Unpowered) != extend {
            return Stage::Canceled;
        }
        let Some(head) = DEVICE_FACES[facing].offset(event.base) else {
            return Stage::Blocked;
        };
        if extend {
            let Ok(moves) = self.piston_push_plan(head, facing) else {
                return Stage::Blocked;
            };
            for (destination, block) in moves.into_iter().rev() {
                self.start_motion(event, destination, block);
            }
            self.start_motion(
                event,
                head,
                Device::Head(facing, event.kind).state(&self.registry),
            );
            self.write_redstone(
                event.base,
                Device::Piston(facing, true, event.kind).state(&self.registry),
            );
        } else {
            if !self.retract_piston(event, head) {
                return Stage::Deferred;
            }
        }
        self.trace_piston(event, Stage::Started, None, None, Some(power));
        Stage::Started
    }

    fn extending_motion(&self, position: BlockPos, facing: usize) -> Option<usize> {
        self.java_redstone
            .as_ref()?
            .motions
            .iter()
            .position(|motion| {
                motion.position == position
                    && motion.event.action == PistonAction::Extend
                    && usize::from(motion.event.facing) == facing
            })
    }

    pub(crate) fn piston_retraction_load(
        &self,
        kind: PistonKind,
        facing: Face,
        head: BlockPos,
    ) -> Result<Option<(BlockPos, BlockStateId)>, PistonObstruction> {
        self.piston_block(head)?;
        let front = facing.offset(head).filter(|front| {
            self.dimension
                .height
                .is_none_or(|height| height.contains(front.y))
        });
        if kind == PistonKind::Sticky
            && let Some(front) = front
        {
            return Ok(Some((front, self.piston_block(front)?)));
        }
        Ok(None)
    }

    /// Prepare the touched cells before writing any part of the retraction.
    fn retract_piston(&mut self, event: PistonEvent, head: BlockPos) -> bool {
        let facing = usize::from(event.facing);
        let Ok(load) = self.piston_retraction_load(event.kind, DEVICE_FACES[facing], head) else {
            return false;
        };
        let mut finishing = None;
        let mut pulling = None;
        if let Some((front, state)) = load {
            if let Some(index) = self.extending_motion(front, facing) {
                finishing = Some(index);
            } else if event.action == PistonAction::Retract && self.piston_pushable(state) {
                pulling = Some((front, state));
            }
        }
        if let Some(index) = finishing {
            let motion = self
                .java_redstone
                .as_mut()
                .unwrap()
                .motions
                .remove(index)
                .unwrap();
            self.finish_motion(motion);
        }
        let logic = self.java_redstone.as_mut().unwrap();
        let before = logic.motions.len();
        logic
            .motions
            .retain(|motion| !(motion.position == head && motion.event.base == event.base));
        let moving_head = before != logic.motions.len();
        if self.read_device(head) == Some(Device::Head(facing, event.kind)) || moving_head {
            self.write_redstone(head, self.dimension.empty_state());
        }
        self.start_motion(
            event,
            event.base,
            Device::Piston(facing, false, event.kind).state(&self.registry),
        );
        if let Some((front, state)) = pulling
            && self
                .dimension
                .voxel(head)
                .is_some_and(|(id, _)| self.dimension.is_empty(id))
            && !self
                .player_body()
                .is_some_and(|(_, player)| player.overlaps(head))
        {
            self.write_redstone(front, self.dimension.empty_state());
            self.start_motion(event, head, state);
        }
        true
    }

    fn start_motion(&mut self, event: PistonEvent, position: BlockPos, result: BlockStateId) {
        let motion = Motion {
            event,
            position,
            result,
            remaining: MOTION_TICKS,
            previous_remaining: MOTION_TICKS,
            awaiting_activation: false,
        };
        self.write_redstone(position, motion.marker().state(&self.registry));
        self.java_redstone
            .as_mut()
            .unwrap()
            .motions
            .push_back(motion);
    }

    fn finish_motion(&mut self, motion: Motion) {
        self.write_redstone(motion.position, motion.result);
        self.trace_piston(
            motion.event,
            PistonTraceStage::Completed,
            None,
            Some(motion.position),
            None,
        );
    }

    pub(crate) fn tick_java_piston_motion(&mut self) {
        let count = self.java_redstone.as_ref().unwrap().motions.len();
        for _ in 0..count {
            let mut motion = self
                .java_redstone
                .as_mut()
                .unwrap()
                .motions
                .pop_front()
                .unwrap();
            if self.dimension.is_active(motion.position.chunk())
                && !std::mem::take(&mut motion.awaiting_activation)
            {
                motion.previous_remaining = motion.remaining;
                motion.remaining -= 1;
            }
            if motion.remaining == 0 {
                self.finish_motion(motion);
                self.dispatch_java_edits();
            } else {
                self.java_redstone
                    .as_mut()
                    .unwrap()
                    .motions
                    .push_back(motion);
            }
        }
    }

    fn trace_piston(
        &mut self,
        event: PistonEvent,
        stage: PistonTraceStage,
        source: Option<BlockPos>,
        moving_position: Option<BlockPos>,
        power: Option<PistonPower>,
    ) {
        let trace = PistonTrace {
            gt: self.tick,
            event,
            stage,
            source,
            moving_position,
            power,
        };
        self.java_redstone.as_mut().unwrap().trace.push(trace);
        self.record_bud(event.base, BudRecordKind::BlockEvent(trace));
    }
}

#[cfg(test)]
mod neighbor_dispatch_tests {
    use super::{Face, neighbor_faces_except};

    #[test]
    fn except_dispatch_preserves_java_order_and_omits_only_the_requested_face() {
        for excluded in Face::ALL {
            let actual: Vec<_> = neighbor_faces_except(excluded).collect();
            let expected: Vec<_> = Face::ALL
                .into_iter()
                .filter(|face| *face != excluded)
                .collect();
            assert_eq!(actual, expected, "excluded {excluded:?}");
            assert_eq!(actual.len(), 5, "excluded {excluded:?}");
            assert!(!actual.contains(&excluded), "excluded {excluded:?}");
        }
    }
}
