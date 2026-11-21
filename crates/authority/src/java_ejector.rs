use rustcraft_world::BlockPos;

use crate::java_block_ticks::{BlockTickPriority, ScheduledBlockTick};
use crate::java_bud::{BudRecordKind, EjectorOutcome};
use crate::java_redstone::{EditNotification, JavaBlockTickKind};
use crate::java_signal::{ActivationPower, DEVICE_FACES, Face, Signal};
use crate::redstone::Device;
use crate::{Effect, ItemStack, Rejection, STACK_LIMIT, SeedScope, Server, SystemRandom};

const DELAY_GT: u64 = 4;
const SLOTS: usize = 9;
const EMPTY_INVENTORY: [Option<ItemStack>; SLOTS] = [None; SLOTS];

macro_rules! ejector_variants {
    ($name:literal, $facing:literal) => {
        [
            concat!("rustcraft_java:", $name, $facing),
            concat!("rustcraft_java:", $name, $facing, "_triggered"),
        ]
    };
}

macro_rules! ejector_names {
    ($name:literal) => {
        [
            ejector_variants!($name, "_south"),
            ejector_variants!($name, "_east"),
            ejector_variants!($name, ""),
            ejector_variants!($name, "_west"),
            ejector_variants!($name, "_down"),
            ejector_variants!($name, "_up"),
        ]
    };
}

const DISPENSERS: [[&str; 2]; 6] = ejector_names!("dispenser");
const DROPPERS: [[&str; 2]; 6] = ejector_names!("dropper");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EjectorKind {
    Dispenser,
    Dropper,
}

impl EjectorKind {
    const ALL: [Self; 2] = [Self::Dispenser, Self::Dropper];

    fn states(self) -> &'static [[&'static str; 2]; 6] {
        match self {
            Self::Dispenser => &DISPENSERS,
            Self::Dropper => &DROPPERS,
        }
    }

    pub(crate) fn callback(self) -> JavaBlockTickKind {
        match self {
            Self::Dispenser => JavaBlockTickKind::Dispenser,
            Self::Dropper => JavaBlockTickKind::Dropper,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ejector {
    pub kind: EjectorKind,
    pub facing: usize,
    pub triggered: bool,
}

impl Ejector {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        EjectorKind::ALL.into_iter().find_map(|kind| {
            kind.states()
                .iter()
                .enumerate()
                .find_map(|(facing, states)| {
                    let index = states.iter().position(|state| *state == name)?;
                    Some(Self {
                        kind,
                        facing,
                        triggered: index != 0,
                    })
                })
        })
    }

    pub(crate) fn name(self) -> &'static str {
        self.kind.states()[self.facing][usize::from(self.triggered)]
    }

    pub(crate) fn item(self) -> Self {
        Self {
            facing: 2,
            triggered: false,
            ..self
        }
    }
}

pub(crate) fn java_ejector_names() -> impl Iterator<Item = &'static str> {
    EjectorKind::ALL
        .into_iter()
        .flat_map(|kind| kind.states().iter().flatten().copied())
}

impl Server {
    pub fn java_ejector_inventory(&self, position: BlockPos) -> Option<[Option<ItemStack>; SLOTS]> {
        if !matches!(self.read_device(position), Some(Device::Ejector(_))) {
            return None;
        }
        let logic = self.java_redstone.as_ref()?;
        Some(
            logic
                .ejector_inventories
                .get(&position)
                .copied()
                .unwrap_or(EMPTY_INVENTORY),
        )
    }

    pub(crate) fn set_java_ejector_inventory(
        &mut self,
        position: BlockPos,
        expected_chunk_revision: u64,
        slots: [Option<ItemStack>; SLOTS],
    ) -> Result<Effect, Rejection> {
        let (_, actual) = self
            .dimension
            .voxel(position)
            .ok_or(Rejection::InactiveChunk)?;
        if actual != expected_chunk_revision {
            return Err(Rejection::StaleChunk { actual });
        }
        let existing = self
            .java_ejector_inventory(position)
            .ok_or(Rejection::InvalidInventoryAction)?;
        if slots
            .into_iter()
            .flatten()
            .any(|stack| !self.valid_java_ejector_stack(stack))
        {
            return Err(Rejection::InvalidInventoryAction);
        }
        if existing == slots {
            return Ok(Effect::Unchanged);
        }
        self.store_java_ejector_inventory(position, slots);
        Ok(Effect::InventoryChanged)
    }

    pub(crate) fn valid_java_ejector_stack(&self, stack: ItemStack) -> bool {
        (1..=STACK_LIMIT).contains(&stack.count)
            && self.registry.block(stack.block).is_some()
            && !self.dimension.is_empty(stack.block)
            && self.device_item(stack.block) == stack.block
    }

    fn store_java_ejector_inventory(
        &mut self,
        position: BlockPos,
        slots: [Option<ItemStack>; SLOTS],
    ) {
        let inventories = &mut self.java_redstone.as_mut().unwrap().ejector_inventories;
        if slots == EMPTY_INVENTORY {
            inventories.remove(&position);
        } else {
            inventories.insert(position, slots);
        }
        let _ = self.notify_java_comparator_output(position);
    }

    pub(crate) fn remove_java_ejector_inventory(&mut self, position: BlockPos) {
        if let Some(slots) = self
            .java_redstone
            .as_mut()
            .unwrap()
            .ejector_inventories
            .remove(&position)
        {
            for item in slots.into_iter().flatten() {
                self.spawn_java_item(item, center(position), [0.0, 0.1, 0.0]);
            }
            let _ = self.notify_java_comparator_output(position);
        }
    }

    pub(crate) fn java_container_signal(&self, position: BlockPos) -> Signal {
        let total: u16 = self
            .java_ejector_inventory(position)
            .into_iter()
            .flatten()
            .flatten()
            .map(|stack| stack.count)
            .sum();
        let value = if total == 0 {
            0
        } else {
            1 + (14 * total) / (SLOTS as u16 * STACK_LIMIT)
        };
        Signal::new(value as u8).unwrap()
    }

    pub(crate) fn java_ejector_activation(&self, position: BlockPos) -> ActivationPower {
        use ActivationPower::{Direct, Quasi, Unavailable, Unpowered};
        let direct = self.java_neighbor_activation(position);
        if matches!(direct, Direct(_)) {
            return direct;
        }
        let above = Face::Up
            .offset(position)
            .map_or(Unavailable, |pos| self.java_neighbor_activation(pos));
        match (direct, above) {
            (_, Direct(source)) => Quasi(source),
            (Unpowered, Unpowered) => Unpowered,
            _ => Unavailable,
        }
    }

    pub(crate) fn update_java_ejector(
        &mut self,
        position: BlockPos,
        ejector: Ejector,
        source: Option<BlockPos>,
    ) {
        let activation = self.java_ejector_activation(position);
        self.record_bud(position, BudRecordKind::Check { source, activation });
        let Some(powered) = activation.powered() else {
            return;
        };
        if powered == ejector.triggered {
            return;
        }
        if powered {
            self.schedule_java_block_tick(
                position,
                ejector.kind.callback(),
                BlockTickPriority::Normal,
                DELAY_GT,
            );
        }
        self.write_java_device_state(
            position,
            Device::Ejector(Ejector {
                triggered: powered,
                ..ejector
            }),
            EditNotification::ShapesOnly,
        );
    }

    pub(crate) fn run_java_ejector(
        &mut self,
        tick: ScheduledBlockTick<JavaBlockTickKind>,
    ) -> EjectorOutcome {
        let source = tick.position;
        let Some(Device::Ejector(ejector)) = self.read_device(source) else {
            return EjectorOutcome::Canceled;
        };
        if ejector.kind.callback() != tick.kind {
            return EjectorOutcome::Canceled;
        }
        let Some(target) = DEVICE_FACES[ejector.facing].offset(source) else {
            return EjectorOutcome::Canceled;
        };
        let mut destination = None;
        if ejector.kind == EjectorKind::Dropper {
            if self
                .dimension
                .height
                .is_some_and(|height| height.contains(target.y))
                && self.dimension.voxel(target).is_none()
            {
                self.java_redstone
                    .as_mut()
                    .unwrap()
                    .block_ticks
                    .schedule(tick);
                return EjectorOutcome::DeferredChunk;
            }
            destination = self.java_ejector_inventory(target);
        }

        let mut slots = self.java_ejector_inventory(source).unwrap();
        let mut occupied = slots
            .iter()
            .enumerate()
            .filter_map(|(slot, stack)| stack.map(|stack| (slot, stack)));
        let count = occupied.clone().count();
        if count == 0 {
            return EjectorOutcome::Empty;
        }
        let Ok(mut random) = self.system_random(SeedScope::World, "java:ejector") else {
            self.java_redstone
                .as_mut()
                .unwrap()
                .block_ticks
                .schedule(tick);
            return EjectorOutcome::DeferredRandom;
        };
        let (slot, stack) = occupied.nth(sample_slot(&mut random, count)).unwrap();
        let item = ItemStack { count: 1, ..stack };
        slots[slot] = (stack.count > 1).then_some(ItemStack {
            count: stack.count - 1,
            ..stack
        });

        if let Some(mut target_slots) = destination {
            random.commit();
            let Some(target_slot) = target_slots.iter_mut().find(|target| {
                target.is_none_or(|stack| stack.block == item.block && stack.count < STACK_LIMIT)
            }) else {
                return EjectorOutcome::TransferBlocked;
            };
            *target_slot = Some(ItemStack {
                count: target_slot.map_or(1, |stack| stack.count + 1),
                ..item
            });
            // Commit both inventories before either source delivers its comparator update.
            let inventories = &mut self.java_redstone.as_mut().unwrap().ejector_inventories;
            inventories.insert(target, target_slots);
            if slots == EMPTY_INVENTORY {
                inventories.remove(&source);
            } else {
                inventories.insert(source, slots);
            }
            let _ = self.notify_java_comparator_output(target);
            let _ = self.notify_java_comparator_output(source);
            EjectorOutcome::Transferred
        } else {
            let velocity = ejection_velocity(&mut random, source, target);
            random.commit();
            let direction = direction(source, target);
            let mut position = center(source);
            for axis in 0..3 {
                position[axis] += 0.7 * direction[axis];
            }
            position[1] -= if direction[1] == 0.0 { 0.15625 } else { 0.125 };
            self.spawn_java_item(item, position, velocity);
            self.store_java_ejector_inventory(source, slots);
            EjectorOutcome::Ejected
        }
    }
}

fn sample_slot(random: &mut SystemRandom<'_>, count: usize) -> usize {
    let count = count as u64;
    let threshold = count.wrapping_neg() % count;
    loop {
        let sample = random.next_u64();
        if sample >= threshold {
            return (sample % count) as usize;
        }
    }
}

fn center(position: BlockPos) -> [f64; 3] {
    [
        position.x as f64 + 0.5,
        position.y as f64 + 0.5,
        position.z as f64 + 0.5,
    ]
}

fn direction(source: BlockPos, target: BlockPos) -> [f64; 3] {
    [
        (target.x - source.x) as f64,
        (target.y - source.y) as f64,
        (target.z - source.z) as f64,
    ]
}

fn ejection_velocity(
    random: &mut SystemRandom<'_>,
    source: BlockPos,
    target: BlockPos,
) -> [f64; 3] {
    let mut unit = || (random.next_u64() >> 11) as f64 * (1.0 / (1_u64 << 53) as f64);
    let speed = 0.2 + unit() * 0.1;
    let travel = direction(source, target);
    [travel[0] * speed, 0.2, travel[2] * speed].map(|mean| mean + (unit() - unit()) * 0.103365)
}
