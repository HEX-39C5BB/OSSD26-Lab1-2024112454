#![forbid(unsafe_code)]

pub mod generation;
mod section;
mod wire;

pub use wire::{WireCell, WireGeometry, stepped_wire};

pub use section::{SECTION_EDGE, SECTION_VOLUME, Section};

use std::collections::BTreeMap;

/// An ID in a frozen runtime registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockStateId(u16);

impl BlockStateId {
    pub const fn from_registry_index(index: u16) -> Self {
        Self(index)
    }

    pub const fn registry_index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockPos {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

/// Same-height neighbors ordered south, east, north, west.
pub const HORIZONTAL_NEIGHBORS: [[i64; 3]; 4] = [[0, 0, 1], [1, 0, 0], [0, 0, -1], [-1, 0, 0]];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ControlMount {
    Wall,
    Floor,
    Ceiling,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlMaterial {
    Stone,
    Oak,
    Gold,
    Iron,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainModel {
    Lever {
        mount: ControlMount,
        facing: usize,
        powered: bool,
    },
    Button {
        mount: ControlMount,
        material: ControlMaterial,
        facing: usize,
        pressed: bool,
    },
    PressurePlate {
        material: ControlMaterial,
        pressed: bool,
    },
    Ejector {
        facing: usize,
        dropper: bool,
    },
    RedstoneTorch {
        wall_facing: Option<usize>,
        lit: bool,
    },
    Repeater {
        facing: usize,
        powered: bool,
        delay_rt: u8,
        locked: bool,
    },
    Comparator {
        facing: usize,
        powered: bool,
        subtract: bool,
    },
    Cake {
        bites: u8,
    },
    Piston {
        facing: usize,
        extended: bool,
        sticky: bool,
    },
    PistonHead {
        facing: usize,
        sticky: bool,
    },
}

impl BlockPos {
    pub const fn new(x: i64, y: i64, z: i64) -> Self {
        Self { x, y, z }
    }

    pub fn chunk(self) -> ChunkPos {
        let edge = SECTION_EDGE as i64;
        ChunkPos {
            x: self.x.div_euclid(edge),
            z: self.z.div_euclid(edge),
        }
    }

    fn section(self) -> (i64, usize) {
        let edge = SECTION_EDGE as i64;
        let x = self.x.rem_euclid(edge) as usize;
        let y = self.y.rem_euclid(edge) as usize;
        let z = self.z.rem_euclid(edge) as usize;
        (
            self.y.div_euclid(edge),
            (y * SECTION_EDGE + z) * SECTION_EDGE + x,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChunkPos {
    pub x: i64,
    pub z: i64,
}

impl ChunkPos {
    pub const ORIGIN: Self = Self { x: 0, z: 0 };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerticalRange {
    pub min: i64,
    pub max_exclusive: i64,
}

impl VerticalRange {
    pub const FLAT_DEFAULT: Self = Self {
        min: -64,
        max_exclusive: 256,
    };

    pub fn contains(self, y: i64) -> bool {
        (self.min..self.max_exclusive).contains(&y)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    position: ChunkPos,
    revision: u64,
    empty: BlockStateId,
    sections: BTreeMap<i64, Section>,
}

impl Chunk {
    pub fn new(position: ChunkPos, empty: BlockStateId) -> Self {
        Self::from_sections(position, empty, BTreeMap::new())
    }

    pub fn from_sections(
        position: ChunkPos,
        empty: BlockStateId,
        sections: BTreeMap<i64, Section>,
    ) -> Self {
        Self {
            position,
            revision: 0,
            empty,
            sections,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn empty_block(&self) -> BlockStateId {
        self.empty
    }

    pub fn position(&self) -> ChunkPos {
        self.position
    }

    pub fn sections(&self) -> &BTreeMap<i64, Section> {
        &self.sections
    }

    pub fn from_checkpoint(
        position: ChunkPos,
        empty: BlockStateId,
        sections: BTreeMap<i64, Section>,
        revision: u64,
    ) -> Self {
        Self {
            position,
            empty,
            sections,
            revision,
        }
    }

    pub fn block(&self, position: BlockPos) -> Option<BlockStateId> {
        if position.chunk() != self.position {
            return None;
        }
        let (section_y, index) = position.section();
        Some(
            self.sections
                .get(&section_y)
                .map_or(self.empty, |section| section.get(index)),
        )
    }

    /// The authority checks block registry membership and the expected revision before editing.
    pub fn set_block(&mut self, position: BlockPos, state: BlockStateId) -> bool {
        let previous = self
            .block(position)
            .expect("position must belong to this chunk");
        if previous == state {
            return false;
        }
        let (section_y, index) = position.section();
        let section = self
            .sections
            .entry(section_y)
            .or_insert_with(|| Section::uniform(self.empty));
        section.set(index, state);
        self.advance_revision();
        true
    }

    pub fn advance_revision(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("chunk revision exhausted");
    }
}
