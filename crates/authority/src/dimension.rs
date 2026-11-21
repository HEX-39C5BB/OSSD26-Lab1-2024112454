use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use rustcraft_world::generation::WorldGenerator;
use rustcraft_world::{BlockPos, BlockStateId, Chunk, ChunkPos, VerticalRange};

use crate::entity::Entities;
use crate::{Player, Rejection};

pub(crate) struct Dimension {
    empty: BlockStateId,
    seed: u64,
    generator: Option<Box<dyn WorldGenerator>>,
    pub(crate) height: Option<VerticalRange>,
    chunks: BTreeMap<ChunkPos, ResidentChunk>,
    pub(crate) entities: Entities,
    pub(crate) redstone_edits: Option<BTreeSet<BlockPos>>,
    pub(crate) java_redstone_edits: Option<VecDeque<crate::java_redstone::Edit>>,
    pub(crate) non_solid: BTreeSet<usize>,
    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    block_updates: Vec<crate::BlockUpdate>,
    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    pub(crate) redstone_updates: Vec<crate::RedstoneUpdate>,
}

#[derive(Clone)]
struct ResidentChunk {
    active: bool,
    data: Arc<Chunk>,
}

impl Dimension {
    pub(crate) fn new(
        empty: BlockStateId,
        seed: u64,
        generator: Option<Box<dyn WorldGenerator>>,
        height: Option<VerticalRange>,
    ) -> Self {
        Self {
            empty,
            seed,
            generator,
            height,
            chunks: BTreeMap::new(),
            entities: Entities::new(),
            redstone_edits: None,
            java_redstone_edits: None,
            non_solid: BTreeSet::new(),
            #[cfg(any(debug_assertions, feature = "dev-tools"))]
            block_updates: Vec::new(),
            #[cfg(any(debug_assertions, feature = "dev-tools"))]
            redstone_updates: Vec::new(),
        }
    }

    pub(crate) fn set_active(&mut self, position: ChunkPos, active: bool) -> bool {
        let resident = if active {
            self.chunks
                .entry(position)
                .or_insert_with(|| ResidentChunk {
                    active: false,
                    data: Arc::new(self.generator.as_ref().map_or_else(
                        || Chunk::new(position, self.empty),
                        |generator| generator.generate(self.seed, position),
                    )),
                })
        } else if let Some(resident) = self.chunks.get_mut(&position) {
            resident
        } else {
            return false;
        };
        if resident.active == active {
            return false;
        }
        resident.active = active;
        Arc::make_mut(&mut resident.data).advance_revision();
        true
    }

    pub(crate) fn edit_block(
        &mut self,
        position: BlockPos,
        state: BlockStateId,
        expected_revision: u64,
    ) -> Result<bool, crate::Rejection> {
        self.edit_block_with_notifications(
            position,
            state,
            expected_revision,
            crate::java_redstone::EditNotification::All,
        )
    }

    pub(crate) fn edit_block_with_notifications(
        &mut self,
        position: BlockPos,
        state: BlockStateId,
        expected_revision: u64,
        notifications: crate::java_redstone::EditNotification,
    ) -> Result<bool, crate::Rejection> {
        if self
            .height
            .is_some_and(|height| !height.contains(position.y))
        {
            return Err(crate::Rejection::OutOfWorld);
        }
        let chunk = self
            .chunks
            .get_mut(&position.chunk())
            .filter(|chunk| chunk.active)
            .ok_or(crate::Rejection::InactiveChunk)?;
        if chunk.data.revision() != expected_revision {
            return Err(crate::Rejection::StaleChunk {
                actual: chunk.data.revision(),
            });
        }
        let before = chunk.data.block(position).ok_or(Rejection::OutOfWorld)?;
        if before == state {
            return Ok(false);
        }
        let changed = Arc::make_mut(&mut chunk.data).set_block(position, state);
        if changed && let Some(edits) = &mut self.redstone_edits {
            edits.insert(position);
        }
        if changed && let Some(edits) = &mut self.java_redstone_edits {
            edits.push_back(crate::java_redstone::Edit {
                position,
                before,
                after: state,
                notifications,
            });
        }
        #[cfg(any(debug_assertions, feature = "dev-tools"))]
        if changed {
            self.block_updates.push(crate::BlockUpdate {
                position,
                before,
                after: state,
            });
        }
        Ok(changed)
    }

    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    pub(crate) fn take_block_updates(&mut self) -> Vec<crate::BlockUpdate> {
        std::mem::take(&mut self.block_updates)
    }

    pub(crate) fn is_active(&self, position: ChunkPos) -> bool {
        self.chunks.get(&position).is_some_and(|chunk| chunk.active)
    }

    pub(crate) fn active_positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.chunks
            .iter()
            .filter_map(|(&position, chunk)| chunk.active.then_some(position))
    }

    pub(crate) fn set_active_checked(
        &mut self,
        position: ChunkPos,
        active: bool,
        player: Option<Player>,
    ) -> Result<bool, Rejection> {
        if !active && player.is_some_and(|player| player.overlaps_chunk(position)) {
            return Err(Rejection::ChunkOccupied);
        }
        Ok(self.set_active(position, active))
    }

    pub(crate) fn voxel(&self, position: BlockPos) -> Option<(BlockStateId, u64)> {
        if self
            .height
            .is_some_and(|height| !height.contains(position.y))
        {
            return None;
        }
        let chunk = self
            .chunks
            .get(&position.chunk())
            .filter(|chunk| chunk.active)?;
        Some((chunk.data.block(position)?, chunk.data.revision()))
    }

    pub(crate) fn is_empty(&self, state: BlockStateId) -> bool {
        state == self.empty
    }

    pub(crate) fn resident_block(&self, position: BlockPos) -> Option<BlockStateId> {
        self.chunks.get(&position.chunk())?.data.block(position)
    }

    pub(crate) fn is_passable(&self, state: BlockStateId) -> bool {
        state == self.empty || self.non_solid.contains(&state.registry_index())
    }

    pub(crate) fn empty_state(&self) -> BlockStateId {
        self.empty
    }

    pub(crate) fn chunks(&self) -> BTreeMap<ChunkPos, Arc<Chunk>> {
        self.chunks
            .iter()
            .filter(|(_, chunk)| chunk.active)
            .map(|(&position, chunk)| (position, Arc::clone(&chunk.data)))
            .collect()
    }

    pub(crate) fn resident_chunks(&self) -> impl Iterator<Item = (bool, &Chunk)> {
        self.chunks
            .values()
            .map(|chunk| (chunk.active, chunk.data.as_ref()))
    }

    pub(crate) fn restore_chunks(&mut self, chunks: Vec<(bool, Chunk)>) {
        self.chunks = chunks
            .into_iter()
            .map(|(active, data)| {
                (
                    data.position(),
                    ResidentChunk {
                        active,
                        data: Arc::new(data),
                    },
                )
            })
            .collect();
    }

    pub(crate) fn prepare_player_location(&mut self, player: Player) -> Result<(), Rejection> {
        let mut activated = Vec::new();
        for position in player.occupied_chunks() {
            if !self.is_active(position) {
                activated.push((position, self.chunks.get(&position).cloned()));
                self.set_active(position, true);
            }
        }
        if player.has_space(self) {
            return Ok(());
        }
        for (position, saved) in activated {
            if let Some(saved) = saved {
                self.chunks.insert(position, saved);
            } else {
                self.chunks.remove(&position);
            }
        }
        Err(Rejection::PlacementBlocked)
    }
}
