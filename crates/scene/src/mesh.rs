use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rustcraft_world::{
    BlockPos, BlockStateId, Chunk, ChunkPos, HORIZONTAL_NEIGHBORS, SECTION_EDGE, SECTION_VOLUME,
    TerrainModel, WireCell, WireGeometry, stepped_wire,
};

const EDGE: usize = SECTION_EDGE;
const SIDE: i64 = SECTION_EDGE as i64;
pub type Chunks = BTreeMap<ChunkPos, Arc<Chunk>>;
const HORIZONTAL: [(i64, i64); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

#[derive(Default)]
pub struct MeshMaterials {
    pub passable: BTreeSet<usize>,
    pub wires: BTreeSet<usize>,
    pub wire_geometry: BTreeMap<usize, WireGeometry>,
    pub ports: BTreeMap<usize, u8>,
    pub models: BTreeMap<usize, TerrainModel>,
}

impl MeshMaterials {
    fn non_occluding(&self, index: usize) -> bool {
        self.passable.contains(&index)
            || matches!(
                self.models.get(&index),
                Some(TerrainModel::Piston { extended: true, .. } | TerrainModel::PistonHead { .. })
            )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Source {
    position: ChunkPos,
    center: u64,
    adjacent: [Option<u64>; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quad {
    /// Horizontal coordinates are chunk local. Vertical coordinates are world-space.
    pub origin: [i64; 3],
    pub axis: usize,
    pub positive: bool,
    pub size: [u8; 2],
    pub block: BlockStateId,
}

#[derive(Clone, Debug)]
pub struct ChunkMesh {
    source: Source,
    pub quads: Vec<Quad>,
    pub connected: Vec<ConnectedSurface>,
    pub models: Vec<([i64; 3], BlockStateId)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectedSurface {
    /// Terrain-space origin with horizontal coordinates relative to the chunk.
    pub origin: [i64; 3],
    pub block: BlockStateId,
    /// Active ports in the order of `HORIZONTAL_NEIGHBORS`.
    pub ports: u8,
    pub ascending: u8,
}

impl ChunkMesh {
    pub fn position(&self) -> ChunkPos {
        self.source.position
    }

    pub fn is_current(&self, chunks: &Chunks) -> bool {
        Source::inspect(chunks, self.source.position).is_some_and(|source| source == self.source)
    }
}

pub struct MeshJob {
    source: Source,
    center: Arc<Chunk>,
    adjacent: [Option<Arc<Chunk>>; 4],
    empty: BlockStateId,
}

impl Source {
    fn inspect(chunks: &Chunks, position: ChunkPos) -> Option<Self> {
        Some(Self {
            position,
            center: chunks.get(&position)?.revision(),
            adjacent: std::array::from_fn(|side| {
                neighbor(chunks, position, side).map(|chunk| chunk.revision())
            }),
        })
    }
}

fn neighbor(chunks: &Chunks, position: ChunkPos, side: usize) -> Option<&Arc<Chunk>> {
    let (x, z) = HORIZONTAL[side];
    chunks.get(&ChunkPos {
        x: position.x.checked_add(x)?,
        z: position.z.checked_add(z)?,
    })
}

impl MeshJob {
    pub fn capture(chunks: &Chunks, position: ChunkPos, empty: BlockStateId) -> Option<Self> {
        let source = Source::inspect(chunks, position)?;
        Some(Self {
            source,
            center: Arc::clone(chunks.get(&position)?),
            adjacent: std::array::from_fn(|side| neighbor(chunks, position, side).cloned()),
            empty,
        })
    }

    pub fn build(self) -> ChunkMesh {
        self.build_with_materials(&MeshMaterials::default())
    }

    pub fn build_with_materials(self, materials: &MeshMaterials) -> ChunkMesh {
        let mut quads = Vec::new();
        let mut connected = Vec::new();
        let mut models = Vec::new();
        for (&section_y, section) in self.center.sections() {
            let Some(base_y) = section_y.checked_mul(SIDE) else {
                continue;
            };
            if (0..SECTION_VOLUME).all(|index| section.get(index) == self.empty) {
                continue;
            }

            if !materials.wires.is_empty() || !materials.models.is_empty() {
                for index in 0..SECTION_VOLUME {
                    let block = section.get(index);
                    let state = block.registry_index();
                    let wire = materials.wires.contains(&state);
                    let model = materials.models.contains_key(&state);
                    if !wire && !model {
                        continue;
                    }
                    let x = (index % EDGE) as i64;
                    let z = (index / EDGE % EDGE) as i64;
                    let local_y = (index / (EDGE * EDGE)) as i64;
                    let Some(y) = base_y.checked_add(local_y) else {
                        continue;
                    };
                    if model {
                        models.push(([x, y, z], block));
                        continue;
                    }
                    if let Some(geometry) = materials.wire_geometry.get(&state) {
                        connected.push(ConnectedSurface {
                            origin: [x, y, z],
                            block,
                            ports: geometry.ports,
                            ascending: geometry.ascending,
                        });
                        continue;
                    }
                    let mut connected_faces = 0;
                    let mut ascending = 0;
                    let cell = |pos: BlockPos| {
                        let state = self.sample_loaded(section_y, [pos.x, pos.y, pos.z])?;
                        Some(if materials.wires.contains(&state.registry_index()) {
                            WireCell::Dust
                        } else if state == self.empty
                            || materials.passable.contains(&state.registry_index())
                        {
                            WireCell::Open
                        } else {
                            WireCell::Solid
                        })
                    };
                    for (direction, [dx, _, dz]) in HORIZONTAL_NEIGHBORS.into_iter().enumerate() {
                        let neighbor = self.sample(section_y, [x + dx, local_y, z + dz]);
                        let neighbor_faces = materials
                            .ports
                            .get(&neighbor.registry_index())
                            .copied()
                            .unwrap_or(0);
                        let bit = 1 << direction;
                        if neighbor_faces & (1 << ((direction + 2) % 4)) != 0 {
                            connected_faces |= bit;
                        } else if let Some((_, up)) =
                            stepped_wire(BlockPos::new(x, local_y, z), direction, cell)
                        {
                            connected_faces |= bit;
                            if up {
                                ascending |= bit;
                            }
                        }
                    }
                    connected.push(ConnectedSurface {
                        origin: [x, y, z],
                        block,
                        ports: connected_faces,
                        ascending,
                    });
                }
            }

            for axis in 0..3 {
                let horizontal = (axis + 1) % 3;
                let vertical = (axis + 2) % 3;
                for positive in [false, true] {
                    let step = if positive { 1 } else { -1 };
                    for layer in 0..EDGE {
                        let mut mask = [None; EDGE * EDGE];
                        for (index, entry) in mask.iter_mut().enumerate() {
                            let mut cell = [0; 3];
                            cell[axis] = layer as i64;
                            cell[horizontal] = (index % EDGE) as i64;
                            cell[vertical] = (index / EDGE) as i64;
                            let block = section.get(index_in_section(cell));
                            if block == self.empty
                                || materials.wires.contains(&block.registry_index())
                                || materials.models.contains_key(&block.registry_index())
                            {
                                continue;
                            }
                            cell[axis] += step;
                            let neighbor = self.sample(section_y, cell);
                            if neighbor == self.empty
                                || materials.non_occluding(neighbor.registry_index())
                            {
                                *entry = Some(block);
                            }
                        }
                        greedy(&mut mask, |u, v, w, h, block| {
                            let mut origin = [0; 3];
                            origin[axis] = layer as i64 + i64::from(positive);
                            origin[horizontal] = u as i64;
                            origin[vertical] = v as i64;
                            if let Some(y) = base_y.checked_add(origin[1]) {
                                origin[1] = y;
                                quads.push(Quad {
                                    origin,
                                    axis,
                                    positive,
                                    size: [w as u8, h as u8],
                                    block,
                                });
                            }
                        });
                    }
                }
            }
        }
        ChunkMesh {
            source: self.source,
            quads,
            connected,
            models,
        }
    }

    fn sample(&self, section_y: i64, cell: [i64; 3]) -> BlockStateId {
        self.sample_loaded(section_y, cell).unwrap_or(self.empty)
    }

    fn sample_loaded(&self, section_y: i64, mut cell: [i64; 3]) -> Option<BlockStateId> {
        let chunk = if cell[0] < 0 {
            self.adjacent[0].as_ref()
        } else if cell[0] >= SIDE {
            self.adjacent[1].as_ref()
        } else if cell[2] < 0 {
            self.adjacent[2].as_ref()
        } else if cell[2] >= SIDE {
            self.adjacent[3].as_ref()
        } else {
            Some(&self.center)
        };
        let y = section_y.checked_add(cell[1].div_euclid(SIDE))?;
        cell = cell.map(|coordinate| coordinate.rem_euclid(SIDE));
        Some(
            chunk?
                .sections()
                .get(&y)
                .map_or(self.empty, |section| section.get(index_in_section(cell))),
        )
    }
}

fn index_in_section([x, y, z]: [i64; 3]) -> usize {
    (y as usize * EDGE + z as usize) * EDGE + x as usize
}

fn greedy(
    mask: &mut [Option<BlockStateId>; EDGE * EDGE],
    mut emit: impl FnMut(usize, usize, usize, usize, BlockStateId),
) {
    for v in 0..EDGE {
        for u in 0..EDGE {
            let Some(block) = mask[v * EDGE + u] else {
                continue;
            };
            let width = (u..EDGE)
                .take_while(|&x| mask[v * EDGE + x] == Some(block))
                .count();
            let height = (v..EDGE)
                .take_while(|&y| {
                    mask[y * EDGE + u..y * EDGE + u + width]
                        .iter()
                        .all(|&item| item == Some(block))
                })
                .count();
            for y in v..v + height {
                mask[y * EDGE + u..y * EDGE + u + width].fill(None);
            }
            emit(u, v, width, height, block);
        }
    }
}
