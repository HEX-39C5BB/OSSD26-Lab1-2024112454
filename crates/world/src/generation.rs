use std::collections::BTreeMap;

use crate::{BlockStateId, Chunk, ChunkPos, SECTION_EDGE, Section};

pub trait WorldGenerator: Send + Sync {
    fn generate(&self, seed: u64, position: ChunkPos) -> Chunk;
}

/// Flat native terrain, generation revision 1. Its surface is at y = 0.
pub struct FlatTerrain {
    pub empty: BlockStateId,
    pub bedrock: BlockStateId,
    pub soil: BlockStateId,
    pub surface: BlockStateId,
}

impl FlatTerrain {
    pub const ID: &str = "rustcraft:flat";
    pub const VERSION: u32 = 1;
}

impl WorldGenerator for FlatTerrain {
    fn generate(&self, seed: u64, position: ChunkPos) -> Chunk {
        let mut upper = Section::uniform(self.soil);
        let edge = SECTION_EDGE;
        for z in 0..edge {
            for x in 0..edge {
                let column = z * edge + x;
                upper.set((edge - 1) * edge * edge + column, self.surface);

                // Sample world coordinates so columns don't depend on chunk generation order.
                let world_x = (position.x as u64)
                    .wrapping_mul(edge as u64)
                    .wrapping_add(x as u64);
                let world_z = (position.z as u64)
                    .wrapping_mul(edge as u64)
                    .wrapping_add(z as u64);
                let mut sample = seed
                    ^ world_x.wrapping_mul(0x9e37_79b9_7f4a_7c15)
                    ^ world_z.wrapping_mul(0xd1b5_4a32_d192_ed03);
                sample = (sample ^ (sample >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
                if sample >> 61 == 0 {
                    upper.set(6 * edge * edge + column, self.bedrock);
                }
            }
        }
        Chunk::from_sections(
            position,
            self.empty,
            BTreeMap::from([(-2, Section::uniform(self.bedrock)), (-1, upper)]),
        )
    }
}

/// The terrain layers of the Java 1.20.1 Classic Flat preset, with structures disabled.
pub struct ClassicSuperflat {
    pub empty: BlockStateId,
    pub bedrock: BlockStateId,
    pub dirt: BlockStateId,
    pub grass: BlockStateId,
}

impl ClassicSuperflat {
    pub const ID: &str = "rustcraft:java_classic_flat";
    pub const VERSION: u32 = 1;
    pub const BOTTOM: i64 = -64;
    pub const SPAWN_Y: i64 = Self::BOTTOM + 4;
}

impl WorldGenerator for ClassicSuperflat {
    fn generate(&self, _seed: u64, position: ChunkPos) -> Chunk {
        let mut section = Section::uniform(self.empty);
        let plane = SECTION_EDGE * SECTION_EDGE;
        for (layer, state) in [self.bedrock, self.dirt, self.dirt, self.grass]
            .into_iter()
            .enumerate()
        {
            for index in layer * plane..(layer + 1) * plane {
                section.set(index, state);
            }
        }
        Chunk::from_sections(
            position,
            self.empty,
            BTreeMap::from([(Self::BOTTOM.div_euclid(SECTION_EDGE as i64), section)]),
        )
    }
}
