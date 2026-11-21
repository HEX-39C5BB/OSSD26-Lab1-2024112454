use std::collections::{BTreeMap, BTreeSet};

use crate::java_block_ticks::ScheduledBlockTick;
use rustcraft_world::{
    BlockPos, BlockStateId, Chunk, ChunkPos, SECTION_EDGE, SECTION_VOLUME, Section, VerticalRange,
};

use crate::{
    DroppedItem, EntityId, GameMode, Inventory, ItemStack, PistonAction, PistonEvent, PistonKind,
    RandomStreamKey, RulePreset, STACK_LIMIT, ScheduledOperation, ScheduledTask, Server,
};

/// Storage-neutral state captured between authoritative ticks.
#[derive(Clone, Debug, PartialEq)]
pub struct Checkpoint {
    pub seed: u64,
    pub tick: u64,
    pub height: VerticalRange,
    pub rules: RulePreset,
    pub mods: Vec<(String, String)>,
    pub chunks: Vec<SavedChunk>,
    pub player: SavedPlayer,
    pub random_streams: Vec<(RandomStreamKey, u64)>,
    pub scheduled_tasks: SavedScheduledTasks,
    pub java_redstone: Option<SavedJavaRedstone>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SavedJavaRedstone {
    pub pending: Vec<PistonEvent>,
    pub moving: Vec<SavedPistonMotion>,
    pub block_ticks: Vec<ScheduledBlockTick<crate::JavaBlockTickKind>>,
    pub next_block_tick_order: u64,
    pub comparator_outputs: Vec<(BlockPos, crate::java_signal::Signal)>,
    pub ejector_inventories: Vec<SavedEjectorInventory>,
    pub dropped_items: Vec<SavedDroppedItem>,
    pub next_entity: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedEjectorInventory {
    pub position: BlockPos,
    pub slots: Vec<(u8, SavedBlock, u16)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SavedDroppedItem {
    pub id: u64,
    pub revision: u64,
    pub block: SavedBlock,
    pub count: u16,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub age: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedPistonMotion {
    pub event: PistonEvent,
    pub position: BlockPos,
    pub result: SavedBlock,
    pub remaining: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedScheduledTasks {
    pub next_id: u64,
    pub pending: Vec<(ScheduledTask, SavedScheduledOperation)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SavedScheduledOperation {
    Event,
    ReplaceBlock {
        position: BlockPos,
        expected: SavedBlock,
        replacement: SavedBlock,
    },
    DeactivateChunk {
        position: ChunkPos,
    },
    RedstoneCheck {
        position: BlockPos,
    },
}

impl Default for SavedScheduledTasks {
    fn default() -> Self {
        Self {
            next_id: 1,
            pending: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SavedBlock {
    pub name: String,
    pub version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedSection {
    pub y: i64,
    pub palette: Vec<SavedBlock>,
    /// Empty for a uniform section. Otherwise one palette index per block.
    pub indices: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedChunk {
    pub position: ChunkPos,
    pub revision: u64,
    pub active: bool,
    pub sections: Vec<SavedSection>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SavedPlayer {
    pub motion: PlayerMotion,
    pub entity_revision: u64,
    pub inventory_revision: u64,
    pub selected: usize,
    pub slots: Vec<Option<(SavedBlock, u16)>>,
    pub carried: Option<(SavedBlock, u16)>,
    pub game_mode: Option<GameMode>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerMotion {
    pub position: [f64; 3],
    pub yaw: f64,
    pub pitch: f64,
    pub vertical_speed: f64,
    pub grounded: bool,
    pub flying: bool,
}

impl Server {
    pub fn checkpoint(&self) -> Result<Checkpoint, String> {
        let height = self
            .dimension
            .height
            .ok_or("checkpoints require a playable world")?;
        let (id, body) = self.player_body().ok_or("local player unavailable")?;
        let (_, inventory) = self
            .player_inventory()
            .map_err(|error| format!("{error:?}"))?;
        let entities = self.dimension.entities.snapshot();
        if entities.iter().any(|entity| {
            !entity.components.is_empty()
                || (entity.id != id
                    && (self.java_redstone.is_none()
                        || !self.dimension.entities.is_item(entity.id)))
        }) {
            return Err(
                "this checkpoint schema supports the local player and built-in Java dropped items"
                    .into(),
            );
        }
        let save_stack =
            |stack: ItemStack| Ok::<_, String>((self.save_block(stack.block)?, stack.count));
        let slots = inventory
            .slots
            .iter()
            .map(|stack| stack.map(save_stack).transpose())
            .collect::<Result<_, _>>()?;
        let carried = inventory.carried.map(save_stack).transpose()?;
        let chunks = self
            .dimension
            .resident_chunks()
            .map(|(active, chunk)| self.save_chunk(active, chunk))
            .collect::<Result<_, _>>()?;
        Ok(Checkpoint {
            height,
            tick: self.tick,
            seed: self.seed,
            rules: self.rules(),
            mods: self.mod_versions(),
            chunks,
            scheduled_tasks: self.save_scheduled_tasks()?,
            java_redstone: self.save_java_redstone()?,
            random_streams: self
                .random_streams
                .iter()
                .map(|(key, &state)| (key.clone(), state))
                .collect(),
            player: SavedPlayer {
                motion: body.motion(),
                entity_revision: self.dimension.entities.revision(id),
                inventory_revision: inventory.revision,
                selected: inventory.selected,
                slots,
                carried,
                game_mode: self.saved_game_mode,
            },
        })
    }

    pub fn restore(checkpoint: Checkpoint) -> Result<Self, String> {
        let mut server =
            Self::playable_with_rules(checkpoint.seed, checkpoint.height, checkpoint.rules)?;
        if server.mod_versions() != checkpoint.mods {
            return Err("checkpoint mod versions differ from the active rule set".into());
        }
        server.tasks =
            server.restore_scheduled_tasks(checkpoint.tick, checkpoint.scheduled_tasks)?;
        if checkpoint.random_streams.len() > crate::random::MAX_STREAMS {
            return Err("random stream limit exceeded in checkpoint".into());
        }
        for (key, state) in checkpoint.random_streams {
            if !key.is_valid() || server.random_streams.insert(key, state).is_some() {
                return Err("invalid or duplicate saved random stream".into());
            }
        }
        let player = checkpoint.player;
        let (slot_count, hotbar_count) = checkpoint.rules.inventory_size();
        if checkpoint.tick == u64::MAX
            || !valid_revision(player.entity_revision)
            || !valid_revision(player.inventory_revision)
            || player.selected >= hotbar_count
            || player.slots.len() != slot_count
            || (checkpoint.rules == RulePreset::NativeV1
                && player.game_mode.is_none()
                && player.carried.is_some())
        {
            return Err("invalid checkpoint counters or selected slot".into());
        }
        let restore_stack = |(block, count): (SavedBlock, u16)| {
            let block = server.resolve_block(&block)?;
            if !(1..=STACK_LIMIT).contains(&count) || server.dimension.is_empty(block) {
                return Err("invalid inventory stack".into());
            }
            Ok(ItemStack { block, count })
        };
        let slots = player
            .slots
            .into_iter()
            .map(|stack| stack.map(restore_stack).transpose())
            .collect::<Result<_, String>>()?;
        let carried = player.carried.map(restore_stack).transpose()?;

        let mut positions = BTreeSet::new();
        let mut moving_markers = BTreeMap::new();
        let mut chunks = Vec::with_capacity(checkpoint.chunks.len());
        for chunk in checkpoint.chunks {
            if !positions.insert(chunk.position) || !valid_revision(chunk.revision) {
                return Err("duplicate chunk or invalid chunk revision".into());
            }
            let mut sections = BTreeMap::new();
            for section in chunk.sections {
                let first_y = section
                    .y
                    .checked_mul(SECTION_EDGE as i64)
                    .ok_or("invalid section height")?;
                let last_y = first_y + (SECTION_EDGE as i64 - 1);
                if first_y >= checkpoint.height.max_exclusive || last_y < checkpoint.height.min {
                    return Err("section is outside the saved dimension".into());
                }
                let data = server.restore_section(&section, checkpoint.height, first_y)?;
                if section.palette.iter().any(|entry| {
                    entry.name.starts_with("rustcraft_java:moving_piston_")
                        || entry
                            .name
                            .starts_with("rustcraft_java:moving_sticky_piston_")
                }) {
                    for index in 0..SECTION_VOLUME {
                        let Some(crate::redstone::Device::Moving(facing, kind)) =
                            server.device(data.get(index))
                        else {
                            continue;
                        };
                        let edge = SECTION_EDGE as i64;
                        let horizontal = |chunk: i64, local: usize| {
                            chunk
                                .checked_mul(edge)
                                .and_then(|base| base.checked_add(local as i64))
                                .ok_or("moving piston is outside the coordinate range")
                        };
                        let position = BlockPos::new(
                            horizontal(chunk.position.x, index % SECTION_EDGE)?,
                            first_y + (index / (SECTION_EDGE * SECTION_EDGE)) as i64,
                            horizontal(chunk.position.z, index / SECTION_EDGE % SECTION_EDGE)?,
                        );
                        moving_markers.insert(position, (facing, kind));
                    }
                }
                if sections.insert(section.y, data).is_some() {
                    return Err("duplicate section".into());
                }
            }
            chunks.push((
                chunk.active,
                Chunk::from_checkpoint(
                    chunk.position,
                    server.dimension.empty_state(),
                    sections,
                    chunk.revision,
                ),
            ));
        }

        let body = server
            .rules
            .movement()
            .restore(player.motion)
            .ok_or("invalid saved player motion")?;
        server.dimension.restore_chunks(chunks);
        server.restore_java_redstone(
            checkpoint.java_redstone,
            moving_markers,
            checkpoint.height,
        )?;
        if !body.has_space(&server.dimension) {
            return Err("saved player is outside active space".into());
        }
        let id = server.player.expect("local player");
        server.dimension.entities.set_player(id, body);
        server.dimension.entities.set_inventory(
            id,
            Inventory {
                slots,
                selected: player.selected,
                revision: player.inventory_revision,
                carried,
            },
        );
        server
            .dimension
            .entities
            .restore_revision(id, player.entity_revision);
        server.tick = checkpoint.tick;
        server.saved_game_mode = player.game_mode;
        Ok(server)
    }

    fn save_java_redstone(&self) -> Result<Option<SavedJavaRedstone>, String> {
        let Some(logic) = &self.java_redstone else {
            return Ok(None);
        };
        if self
            .dimension
            .java_redstone_edits
            .as_ref()
            .is_some_and(|pending| !pending.is_empty())
        {
            return Err("checkpoint requires the Java block edit queue to be drained".into());
        }
        let moving = logic
            .motions
            .iter()
            .map(|motion| {
                Ok(SavedPistonMotion {
                    event: motion.event,
                    position: motion.position,
                    result: self.save_block(motion.result)?,
                    remaining: if self.rules().uses_java_piston_save() {
                        motion.previous_remaining
                    } else {
                        motion.remaining
                    },
                })
            })
            .collect::<Result<_, String>>()?;
        let ejector_inventories = logic
            .ejector_inventories
            .iter()
            .map(|(&position, slots)| {
                let slots = slots
                    .iter()
                    .enumerate()
                    .filter_map(|(index, stack)| Some((index as u8, stack.as_ref()?)))
                    .map(|(index, stack)| Ok((index, self.save_block(stack.block)?, stack.count)))
                    .collect::<Result<_, String>>()?;
                Ok(SavedEjectorInventory { position, slots })
            })
            .collect::<Result<_, String>>()?;
        let dropped_items = self
            .dimension
            .entities
            .items()
            .map(|(id, body)| {
                Ok(SavedDroppedItem {
                    id: id.persistent_id(),
                    revision: self.dimension.entities.revision(id),
                    block: self.save_block(body.stack.block)?,
                    count: body.stack.count,
                    position: body.position,
                    velocity: body.velocity,
                    age: body.age,
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(Some(SavedJavaRedstone {
            pending: if self.rules().uses_java_piston_save() {
                Vec::new()
            } else {
                logic.pending().collect()
            },
            moving,
            block_ticks: logic.block_ticks.pending().collect(),
            next_block_tick_order: logic.next_block_tick_order,
            comparator_outputs: logic
                .comparator_outputs
                .iter()
                .map(|(&position, &signal)| (position, signal))
                .collect(),
            ejector_inventories,
            dropped_items,
            next_entity: Some(self.next_entity),
        }))
    }

    fn restore_java_redstone(
        &mut self,
        saved: Option<SavedJavaRedstone>,
        mut markers: BTreeMap<BlockPos, (usize, PistonKind)>,
        height: VerticalRange,
    ) -> Result<(), String> {
        use crate::java_redstone::{JavaRedstone, MOTION_TICKS, Motion};
        use crate::java_signal::DEVICE_FACES;
        use crate::redstone::Device;
        let Some(saved) = saved else {
            return if self.java_redstone.is_none() && markers.is_empty() {
                Ok(())
            } else {
                Err("missing Java redstone state".into())
            };
        };
        if self.java_redstone.is_none() {
            return Err("unexpected Java redstone state".into());
        }
        let valid_event = |event: PistonEvent| {
            usize::from(event.facing) < DEVICE_FACES.len() && height.contains(event.base.y)
        };
        let mut logic = JavaRedstone::default();
        for inventory in saved.ejector_inventories {
            let device = self
                .dimension
                .resident_block(inventory.position)
                .and_then(|state| self.device(state));
            if !height.contains(inventory.position.y)
                || inventory.slots.is_empty()
                || !matches!(device, Some(Device::Ejector(_)))
            {
                return Err("invalid Java ejector inventory position".into());
            }
            let mut slots = [None; 9];
            for (index, block, count) in inventory.slots {
                let Some(slot) = slots.get_mut(usize::from(index)) else {
                    return Err("invalid Java ejector slot number".into());
                };
                let stack = ItemStack {
                    block: self.resolve_block(&block)?,
                    count,
                };
                if slot.is_some() || !self.valid_java_ejector_stack(stack) {
                    return Err("invalid or duplicate Java ejector slot".into());
                }
                *slot = Some(stack);
            }
            if logic
                .ejector_inventories
                .insert(inventory.position, slots)
                .is_some()
            {
                return Err("duplicate Java ejector inventory".into());
            }
        }
        if saved.next_block_tick_order == u64::MAX {
            return Err("exhausted Java block tick sequence".into());
        }
        logic.next_block_tick_order = saved.next_block_tick_order;
        for (position, signal) in saved.comparator_outputs {
            use crate::java_signal::Signal;
            let comparator = self
                .dimension
                .resident_block(position)
                .and_then(|state| self.device(state));
            if !height.contains(position.y)
                || !matches!(comparator, Some(Device::Comparator(_)))
                || signal == Signal::OFF
                || logic.comparator_outputs.insert(position, signal).is_some()
            {
                return Err("invalid or duplicate Java comparator output".into());
            }
        }
        let mut block_tick_orders = BTreeSet::new();
        for tick in saved.block_ticks {
            if tick.sub_tick_order >= saved.next_block_tick_order
                || !height.contains(tick.position.y)
                || tick.due_gt == u64::MAX
                || !tick.kind.allows_priority(tick.priority)
                || !block_tick_orders.insert(tick.sub_tick_order)
                || !logic.block_ticks.schedule(tick)
            {
                return Err("invalid or duplicate Java block tick".into());
            }
        }
        if self.rules().uses_java_piston_save() && !saved.pending.is_empty() {
            return Err("Java 1.20.1 Block Events cannot appear in this checkpoint preset".into());
        }
        for event in saved.pending {
            if !valid_event(event) || !logic.enqueue(event) {
                return Err("invalid or duplicate Java piston event".into());
            }
        }
        let max_saved_motion = if self.rules().uses_java_piston_save() {
            MOTION_TICKS
        } else {
            MOTION_TICKS - 1
        };
        for motion in saved.moving {
            if !valid_event(motion.event) || !(1..=max_saved_motion).contains(&motion.remaining) {
                return Err("invalid Java piston motion".into());
            }
            let facing = usize::from(motion.event.facing);
            let kind = motion.event.kind;
            let direction = DEVICE_FACES[facing];
            let head = direction.offset(motion.event.base);
            let result = self.resolve_block(&motion.result)?;
            let valid_result = match motion.event.action {
                PistonAction::Retract | PistonAction::Drop => {
                    if motion.event.base == motion.position {
                        self.device(result) == Some(Device::Piston(facing, false, kind))
                    } else {
                        kind == PistonKind::Sticky
                            && motion.event.action == PistonAction::Retract
                            && head == Some(motion.position)
                            && self.piston_pushable(result)
                    }
                }
                PistonAction::Extend => {
                    if head == Some(motion.position) {
                        self.device(result) == Some(Device::Head(facing, kind))
                    } else {
                        std::iter::successors(head, |pos| direction.offset(*pos))
                            .take(13)
                            .skip(1)
                            .any(|pos| pos == motion.position)
                            && self.piston_pushable(result)
                    }
                }
            };
            if !valid_result {
                return Err("invalid Java piston motion destination or result".into());
            }
            let motion = Motion {
                event: motion.event,
                position: motion.position,
                result,
                remaining: motion.remaining,
                previous_remaining: motion.remaining,
                // The first active tick after loading registers restored piston block entities.
                awaiting_activation: self.rules().uses_java_piston_save(),
            };
            let Some((facing, kind)) = markers.remove(&motion.position) else {
                return Err("Java piston motion does not match its block".into());
            };
            if motion.marker() != Device::Moving(facing, kind) {
                return Err("Java piston motion does not match its block".into());
            }
            logic.motions.push_back(motion);
        }
        if !markers.is_empty() {
            return Err("moving piston is missing its Java motion record".into());
        }
        self.restore_java_items(saved.next_entity, saved.dropped_items)?;
        self.java_redstone = Some(logic);
        Ok(())
    }

    fn restore_java_items(
        &mut self,
        next_entity: Option<u64>,
        items: Vec<SavedDroppedItem>,
    ) -> Result<(), String> {
        let next_entity = next_entity.unwrap_or(self.next_entity);
        let player = self.player.expect("built-in player").persistent_id();
        if next_entity <= player || next_entity == u64::MAX {
            return Err("invalid Java entity sequence".into());
        }
        let mut ids = BTreeSet::new();
        for item in items {
            let stack = ItemStack {
                block: self.resolve_block(&item.block)?,
                count: item.count,
            };
            if !(player + 1..next_entity).contains(&item.id)
                || !ids.insert(item.id)
                || !valid_revision(item.revision)
                || !self.valid_java_ejector_stack(stack)
                || item.age >= crate::java_items::LIFETIME_GT
                || item
                    .position
                    .iter()
                    .any(|value| !value.is_finite() || value.abs() >= (1_u64 << 50) as f64)
                || item
                    .velocity
                    .iter()
                    .any(|value| !value.is_finite() || value.abs() > 10.0)
            {
                return Err("invalid or duplicate Java dropped item".into());
            }
            let id = EntityId::from_persistent_id(item.id);
            self.dimension.entities.spawn_item(
                id,
                DroppedItem {
                    stack,
                    position: item.position,
                    velocity: item.velocity,
                    age: item.age,
                },
            );
            self.dimension.entities.restore_revision(id, item.revision);
        }
        self.next_entity = next_entity;
        Ok(())
    }

    fn mod_versions(&self) -> Vec<(String, String)> {
        self.locked_mods
            .iter()
            .map(|module| (module.id.into(), module.version.to_string()))
            .collect()
    }

    fn save_scheduled_tasks(&self) -> Result<SavedScheduledTasks, String> {
        let pending = self
            .tasks
            .pending
            .iter()
            .map(|(&task, &operation)| {
                let saved = match operation {
                    ScheduledOperation::Event => SavedScheduledOperation::Event,
                    ScheduledOperation::RedstoneCheck { position } => {
                        SavedScheduledOperation::RedstoneCheck { position }
                    }
                    ScheduledOperation::DeactivateChunk { position } => {
                        SavedScheduledOperation::DeactivateChunk { position }
                    }
                    ScheduledOperation::ReplaceBlock {
                        position,
                        expected,
                        replacement,
                    } => SavedScheduledOperation::ReplaceBlock {
                        position,
                        expected: self.save_block(expected)?,
                        replacement: self.save_block(replacement)?,
                    },
                };
                Ok((task, saved))
            })
            .collect::<Result<_, String>>()?;
        Ok(SavedScheduledTasks {
            next_id: self.tasks.next_id,
            pending,
        })
    }

    fn restore_scheduled_tasks(
        &self,
        tick: u64,
        saved: SavedScheduledTasks,
    ) -> Result<crate::tasks::TaskQueue, String> {
        let pending = saved
            .pending
            .into_iter()
            .map(|(task, operation)| {
                let operation = match operation {
                    SavedScheduledOperation::Event => ScheduledOperation::Event,
                    SavedScheduledOperation::RedstoneCheck { position } => {
                        ScheduledOperation::RedstoneCheck { position }
                    }
                    SavedScheduledOperation::DeactivateChunk { position } => {
                        ScheduledOperation::DeactivateChunk { position }
                    }
                    SavedScheduledOperation::ReplaceBlock {
                        position,
                        expected,
                        replacement,
                    } => ScheduledOperation::ReplaceBlock {
                        position,
                        expected: self.resolve_block(&expected)?,
                        replacement: self.resolve_block(&replacement)?,
                    },
                };
                self.validate_scheduled_operation(operation)?;
                Ok((task, operation))
            })
            .collect::<Result<_, String>>()?;
        crate::tasks::TaskQueue::restore(tick, saved.next_id, pending)
    }

    fn save_block(&self, id: BlockStateId) -> Result<SavedBlock, String> {
        let schema = self
            .registry
            .block(id)
            .ok_or("unregistered block in state")?;
        Ok(SavedBlock {
            name: schema.name.into(),
            version: schema.version,
        })
    }

    fn resolve_block(&self, block: &SavedBlock) -> Result<BlockStateId, String> {
        let id = self
            .registry
            .block_id(&block.name)
            .ok_or_else(|| format!("unknown saved block {}", block.name))?;
        if self
            .registry
            .block(id)
            .is_some_and(|schema| schema.version == block.version)
        {
            Ok(id)
        } else {
            Err(format!(
                "unsupported block schema {} version {}",
                block.name, block.version
            ))
        }
    }

    fn save_chunk(&self, active: bool, chunk: &Chunk) -> Result<SavedChunk, String> {
        let mut sections = Vec::with_capacity(chunk.sections().len());
        for (&y, section) in chunk.sections() {
            let mut palette = Vec::new();
            let mut indices = Vec::new();
            if let Section::Uniform(block) = section {
                palette.push(self.save_block(*block)?);
            } else {
                let mut keys = BTreeMap::new();
                for index in 0..SECTION_VOLUME {
                    let block = section.get(index);
                    let slot = match keys.entry(block.registry_index()) {
                        std::collections::btree_map::Entry::Occupied(entry) => *entry.get(),
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            let index = palette.len() as u16;
                            palette.push(self.save_block(block)?);
                            *entry.insert(index)
                        }
                    };
                    indices.push(slot);
                }
                if palette.len() == 1 {
                    indices.clear();
                }
            }
            sections.push(SavedSection {
                y,
                palette,
                indices,
            });
        }
        Ok(SavedChunk {
            position: chunk.position(),
            revision: chunk.revision(),
            active,
            sections,
        })
    }

    fn restore_section(
        &self,
        section: &SavedSection,
        height: VerticalRange,
        first_y: i64,
    ) -> Result<Section, String> {
        let palette = section
            .palette
            .iter()
            .map(|block| self.resolve_block(block))
            .collect::<Result<Vec<_>, _>>()?;
        let Some(&first) = palette.first() else {
            return Err("section palette is empty".into());
        };
        if palette.len() == 1 && section.indices.is_empty() {
            if self.dimension.is_empty(first)
                || (height.contains(first_y)
                    && height.contains(first_y + (SECTION_EDGE as i64 - 1)))
            {
                return Ok(Section::uniform(first));
            }
        } else if palette.len() > 1
            && palette.len() <= SECTION_VOLUME
            && section.indices.len() == SECTION_VOLUME
        {
            let mut result = Section::uniform(first);
            for (index, slot) in section.indices.iter().enumerate() {
                let &id = palette
                    .get(usize::from(*slot))
                    .ok_or("section palette index is invalid")?;
                let y = first_y + (index / (SECTION_EDGE * SECTION_EDGE)) as i64;
                if !height.contains(y) && !self.dimension.is_empty(id) {
                    return Err("section contains blocks outside the saved dimension".into());
                }
                result.set(index, id);
            }
            return Ok(result);
        }
        Err("invalid section encoding".into())
    }
}

fn valid_revision(revision: u64) -> bool {
    (1..u64::MAX).contains(&revision)
}
