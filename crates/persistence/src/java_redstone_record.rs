use std::io;

use rustcraft_authority::checkpoint::{
    SavedBlock, SavedDroppedItem, SavedEjectorInventory, SavedJavaRedstone, SavedPistonMotion,
};
use rustcraft_authority::java_block_ticks::{BlockTickPriority, ScheduledBlockTick};
use rustcraft_authority::java_signal::Signal;
use rustcraft_authority::{JavaBlockTickKind, PistonAction, PistonEvent, PistonKind};
use rustcraft_world::BlockPos;
use serde::{Deserialize, Serialize};

use crate::records;

const LEGACY_SCHEMA: u64 = 1;
const PISTON_KIND_SCHEMA: u64 = 2;
const COMPARATOR_SCHEMA: u64 = 3;
const SCHEMA: u64 = 4;

#[derive(Serialize, Deserialize)]
pub(crate) struct JavaRedstoneRecord {
    schema: u64,
    pending: Vec<EventRecord>,
    moving: Vec<MotionRecord>,
    block_ticks: Vec<BlockTickRecord>,
    next_block_tick_order: u64,
    #[serde(default)]
    comparator_outputs: Vec<ComparatorOutputRecord>,
    #[serde(default)]
    ejector_inventories: Vec<EjectorInventoryRecord>,
    #[serde(default)]
    dropped_items: Vec<DroppedItemRecord>,
    #[serde(default)]
    next_entity: Option<u64>,
}

#[derive(Serialize, Deserialize)]
struct EjectorInventoryRecord {
    position: [i64; 3],
    slots: Vec<EjectorStackRecord>,
}

#[derive(Serialize, Deserialize)]
struct EjectorStackRecord {
    slot: u8,
    block: String,
    version: u32,
    count: u16,
}

#[derive(Serialize, Deserialize)]
struct DroppedItemRecord {
    id: u64,
    revision: u64,
    block: String,
    version: u32,
    count: u16,
    position: [f64; 3],
    velocity: [f64; 3],
    age: u16,
}

#[derive(Serialize, Deserialize)]
struct ComparatorOutputRecord {
    position: [i64; 3],
    strength: u8,
}

#[derive(Serialize, Deserialize)]
struct BlockTickRecord {
    position: [i64; 3],
    kind: BlockTickKindRecord,
    due_gt: u64,
    priority: i8,
    sub_tick_order: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BlockTickKindRecord {
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

impl BlockTickRecord {
    fn kind(&self) -> JavaBlockTickKind {
        match self.kind {
            BlockTickKindRecord::Lamp => JavaBlockTickKind::Lamp,
            BlockTickKindRecord::Repeater => JavaBlockTickKind::Repeater,
            BlockTickKindRecord::Comparator => JavaBlockTickKind::Comparator,
            BlockTickKindRecord::Observer => JavaBlockTickKind::Observer,
            BlockTickKindRecord::RedstoneTorch => JavaBlockTickKind::RedstoneTorch,
            BlockTickKindRecord::RedstoneWallTorch => JavaBlockTickKind::RedstoneWallTorch,
            BlockTickKindRecord::StoneButton => JavaBlockTickKind::StoneButton,
            BlockTickKindRecord::OakButton => JavaBlockTickKind::OakButton,
            BlockTickKindRecord::StonePressurePlate => JavaBlockTickKind::StonePressurePlate,
            BlockTickKindRecord::OakPressurePlate => JavaBlockTickKind::OakPressurePlate,
            BlockTickKindRecord::LightPressurePlate => JavaBlockTickKind::LightPressurePlate,
            BlockTickKindRecord::HeavyPressurePlate => JavaBlockTickKind::HeavyPressurePlate,
            BlockTickKindRecord::Dispenser => JavaBlockTickKind::Dispenser,
            BlockTickKindRecord::Dropper => JavaBlockTickKind::Dropper,
        }
    }

    fn priority(&self) -> Option<BlockTickPriority> {
        let priority = match self.priority {
            -3 => BlockTickPriority::ExtremelyHigh,
            -2 => BlockTickPriority::VeryHigh,
            -1 => BlockTickPriority::High,
            0 => BlockTickPriority::Normal,
            _ => return None,
        };
        self.kind().allows_priority(priority).then_some(priority)
    }
}

#[derive(Serialize, Deserialize)]
struct MotionRecord {
    event: EventRecord,
    position: [i64; 3],
    block: String,
    version: u32,
    remaining: u8,
}

#[derive(Serialize, Deserialize)]
struct EventRecord {
    base: [i64; 3],
    facing: u8,
    action: ActionRecord,
    #[serde(default)]
    kind: Option<PistonKindRecord>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PistonKindRecord {
    Normal,
    Sticky,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ActionRecord {
    Extend,
    Retract,
    Drop,
}

impl JavaRedstoneRecord {
    pub(crate) fn validate(&self) -> io::Result<()> {
        if !matches!(
            self.schema,
            LEGACY_SCHEMA | PISTON_KIND_SCHEMA | COMPARATOR_SCHEMA
        ) {
            records::check_version(self.schema, SCHEMA, "Java redstone")?;
        }
        for event in self
            .pending
            .iter()
            .chain(self.moving.iter().map(|motion| &motion.event))
        {
            if self.schema == LEGACY_SCHEMA {
                if event.kind.is_some() || matches!(event.action, ActionRecord::Drop) {
                    return Err(crate::invalid(
                        "legacy Java redstone record has a new piston event",
                    ));
                }
            } else if event.kind.is_none() {
                return Err(crate::invalid("missing Java piston kind"));
            }
        }
        if self.block_ticks.iter().any(|tick| {
            tick.priority().is_none()
                || (self.schema == LEGACY_SCHEMA && !matches!(tick.kind, BlockTickKindRecord::Lamp))
                || (self.schema < COMPARATOR_SCHEMA
                    && matches!(
                        tick.kind,
                        BlockTickKindRecord::Comparator
                            | BlockTickKindRecord::StoneButton
                            | BlockTickKindRecord::OakButton
                            | BlockTickKindRecord::StonePressurePlate
                            | BlockTickKindRecord::OakPressurePlate
                            | BlockTickKindRecord::LightPressurePlate
                            | BlockTickKindRecord::HeavyPressurePlate
                    ))
                || (self.schema < SCHEMA
                    && matches!(
                        tick.kind,
                        BlockTickKindRecord::Dispenser | BlockTickKindRecord::Dropper
                    ))
        }) {
            return Err(crate::invalid("invalid Java block tick kind or priority"));
        }
        if (self.schema < COMPARATOR_SCHEMA && !self.comparator_outputs.is_empty())
            || self
                .comparator_outputs
                .iter()
                .any(|output| !(1..=15).contains(&output.strength))
        {
            return Err(crate::invalid("invalid Java comparator output record"));
        }
        if (self.schema == SCHEMA) != self.next_entity.is_some()
            || (self.schema < SCHEMA
                && (!self.ejector_inventories.is_empty() || !self.dropped_items.is_empty()))
        {
            return Err(crate::invalid(
                "invalid Java ejector or entity record schema",
            ));
        }
        Ok(())
    }
}

impl From<SavedJavaRedstone> for JavaRedstoneRecord {
    fn from(saved: SavedJavaRedstone) -> Self {
        Self {
            schema: SCHEMA,
            pending: saved.pending.into_iter().map(EventRecord::from).collect(),
            next_block_tick_order: saved.next_block_tick_order,
            block_ticks: saved
                .block_ticks
                .into_iter()
                .map(|tick| BlockTickRecord {
                    position: coords(tick.position),
                    kind: match tick.kind {
                        JavaBlockTickKind::Lamp => BlockTickKindRecord::Lamp,
                        JavaBlockTickKind::Repeater => BlockTickKindRecord::Repeater,
                        JavaBlockTickKind::Comparator => BlockTickKindRecord::Comparator,
                        JavaBlockTickKind::Observer => BlockTickKindRecord::Observer,
                        JavaBlockTickKind::RedstoneTorch => BlockTickKindRecord::RedstoneTorch,
                        JavaBlockTickKind::RedstoneWallTorch => {
                            BlockTickKindRecord::RedstoneWallTorch
                        }
                        JavaBlockTickKind::StoneButton => BlockTickKindRecord::StoneButton,
                        JavaBlockTickKind::OakButton => BlockTickKindRecord::OakButton,
                        JavaBlockTickKind::StonePressurePlate => {
                            BlockTickKindRecord::StonePressurePlate
                        }
                        JavaBlockTickKind::OakPressurePlate => {
                            BlockTickKindRecord::OakPressurePlate
                        }
                        JavaBlockTickKind::LightPressurePlate => {
                            BlockTickKindRecord::LightPressurePlate
                        }
                        JavaBlockTickKind::HeavyPressurePlate => {
                            BlockTickKindRecord::HeavyPressurePlate
                        }
                        JavaBlockTickKind::Dispenser => BlockTickKindRecord::Dispenser,
                        JavaBlockTickKind::Dropper => BlockTickKindRecord::Dropper,
                    },
                    due_gt: tick.due_gt,
                    priority: tick.priority as i8,
                    sub_tick_order: tick.sub_tick_order,
                })
                .collect(),
            moving: saved
                .moving
                .into_iter()
                .map(|motion| MotionRecord {
                    event: motion.event.into(),
                    position: coords(motion.position),
                    block: motion.result.name,
                    version: motion.result.version,
                    remaining: motion.remaining,
                })
                .collect(),
            comparator_outputs: saved
                .comparator_outputs
                .into_iter()
                .map(|(position, signal)| ComparatorOutputRecord {
                    position: coords(position),
                    strength: signal.get(),
                })
                .collect(),
            next_entity: saved.next_entity,
            ejector_inventories: saved
                .ejector_inventories
                .into_iter()
                .map(|inventory| EjectorInventoryRecord {
                    position: coords(inventory.position),
                    slots: inventory
                        .slots
                        .into_iter()
                        .map(|(slot, block, count)| EjectorStackRecord {
                            slot,
                            block: block.name,
                            version: block.version,
                            count,
                        })
                        .collect(),
                })
                .collect(),
            dropped_items: saved
                .dropped_items
                .into_iter()
                .map(|item| DroppedItemRecord {
                    id: item.id,
                    revision: item.revision,
                    block: item.block.name,
                    version: item.block.version,
                    count: item.count,
                    position: item.position,
                    velocity: item.velocity,
                    age: item.age,
                })
                .collect(),
        }
    }
}

impl From<JavaRedstoneRecord> for SavedJavaRedstone {
    fn from(record: JavaRedstoneRecord) -> Self {
        Self {
            next_entity: record.next_entity,
            ejector_inventories: record
                .ejector_inventories
                .into_iter()
                .map(|inventory| SavedEjectorInventory {
                    position: position(inventory.position),
                    slots: inventory
                        .slots
                        .into_iter()
                        .map(|stack| {
                            (
                                stack.slot,
                                SavedBlock {
                                    name: stack.block,
                                    version: stack.version,
                                },
                                stack.count,
                            )
                        })
                        .collect(),
                })
                .collect(),
            dropped_items: record
                .dropped_items
                .into_iter()
                .map(|item| SavedDroppedItem {
                    id: item.id,
                    revision: item.revision,
                    block: SavedBlock {
                        name: item.block,
                        version: item.version,
                    },
                    count: item.count,
                    position: item.position,
                    velocity: item.velocity,
                    age: item.age,
                })
                .collect(),
            pending: record.pending.into_iter().map(PistonEvent::from).collect(),
            comparator_outputs: record
                .comparator_outputs
                .into_iter()
                .map(|output| {
                    (
                        position(output.position),
                        Signal::new(output.strength).expect("validated comparator power"),
                    )
                })
                .collect(),
            next_block_tick_order: record.next_block_tick_order,
            block_ticks: record
                .block_ticks
                .into_iter()
                .map(|tick| ScheduledBlockTick {
                    position: position(tick.position),
                    kind: tick.kind(),
                    due_gt: tick.due_gt,
                    priority: tick.priority().expect("validated Java block tick"),
                    sub_tick_order: tick.sub_tick_order,
                })
                .collect(),
            moving: record
                .moving
                .into_iter()
                .map(|motion| SavedPistonMotion {
                    event: motion.event.into(),
                    position: position(motion.position),
                    result: SavedBlock {
                        name: motion.block,
                        version: motion.version,
                    },
                    remaining: motion.remaining,
                })
                .collect(),
        }
    }
}

impl From<PistonEvent> for EventRecord {
    fn from(event: PistonEvent) -> Self {
        Self {
            base: coords(event.base),
            facing: event.facing,
            action: match event.action {
                PistonAction::Extend => ActionRecord::Extend,
                PistonAction::Retract => ActionRecord::Retract,
                PistonAction::Drop => ActionRecord::Drop,
            },
            kind: Some(match event.kind {
                PistonKind::Normal => PistonKindRecord::Normal,
                PistonKind::Sticky => PistonKindRecord::Sticky,
            }),
        }
    }
}

impl From<EventRecord> for PistonEvent {
    fn from(record: EventRecord) -> Self {
        Self {
            base: position(record.base),
            facing: record.facing,
            action: match record.action {
                ActionRecord::Extend => PistonAction::Extend,
                ActionRecord::Retract => PistonAction::Retract,
                ActionRecord::Drop => PistonAction::Drop,
            },
            kind: match record.kind.unwrap_or(PistonKindRecord::Normal) {
                PistonKindRecord::Normal => PistonKind::Normal,
                PistonKindRecord::Sticky => PistonKind::Sticky,
            },
        }
    }
}

fn coords(pos: BlockPos) -> [i64; 3] {
    [pos.x, pos.y, pos.z]
}

fn position([x, y, z]: [i64; 3]) -> BlockPos {
    BlockPos::new(x, y, z)
}
