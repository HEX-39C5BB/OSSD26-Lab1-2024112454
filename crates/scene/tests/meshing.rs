use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rustcraft_scene::mesh::{Chunks, MeshJob, MeshMaterials};
use rustcraft_world::{BlockPos, BlockStateId, Chunk, ChunkPos, Section, WireGeometry};

const AIR: BlockStateId = BlockStateId::from_registry_index(0);
const ROCK: BlockStateId = BlockStateId::from_registry_index(1);

#[test]
fn meshes_wire_from_its_committed_ports_and_vertical_segments() {
    let position = ChunkPos { x: 0, z: 0 };
    let dot = BlockStateId::from_registry_index(2);
    let step = BlockStateId::from_registry_index(3);
    let mut center = Chunk::new(position, AIR);
    center.set_block(BlockPos::new(14, 0, 5), dot);
    center.set_block(BlockPos::new(15, 0, 5), step);
    let materials = MeshMaterials {
        wires: BTreeSet::from([dot.registry_index(), step.registry_index()]),
        wire_geometry: BTreeMap::from([
            (dot.registry_index(), WireGeometry::default()),
            (
                step.registry_index(),
                WireGeometry {
                    ports: 0b1010,
                    ascending: 0b0010,
                },
            ),
        ]),
        ..Default::default()
    };
    let chunks = BTreeMap::from([(position, Arc::new(center))]);
    let mesh = MeshJob::capture(&chunks, position, AIR)
        .unwrap()
        .build_with_materials(&materials);
    let actual: Vec<_> = mesh
        .connected
        .iter()
        .map(|wire| (wire.block, wire.ports, wire.ascending))
        .collect();
    assert_eq!(actual, [(dot, 0, 0), (step, 0b1010, 0b0010)]);
}

fn solid(position: ChunkPos, levels: &[i64]) -> Arc<Chunk> {
    Arc::new(Chunk::from_sections(
        position,
        AIR,
        levels
            .iter()
            .map(|&y| (y, Section::uniform(ROCK)))
            .collect(),
    ))
}

#[test]
fn greedily_merges_and_culls_faces_between_chunks_and_sections() {
    let origin = ChunkPos::ORIGIN;
    let mut chunks: Chunks = BTreeMap::from([(origin, solid(origin, &[0]))]);
    let mesh = MeshJob::capture(&chunks, origin, AIR).unwrap().build();
    assert_eq!(mesh.quads.len(), 6);
    assert!(mesh.quads.iter().all(|quad| quad.size == [16, 16]));

    let right = ChunkPos { x: 1, z: 0 };
    chunks.insert(right, solid(right, &[0]));
    chunks.insert(origin, solid(origin, &[-1, 0]));
    let mesh = MeshJob::capture(&chunks, origin, AIR).unwrap().build();
    assert_eq!(mesh.quads.len(), 9);
    assert!(
        !mesh
            .quads
            .iter()
            .any(|q| q.axis == 0 && q.positive && q.origin[1] == 0)
    );
    assert!(!mesh.quads.iter().any(|q| q.axis == 1 && q.origin[1] == 0));
}

#[test]
fn rejects_late_meshes_when_a_seam_changes() {
    let origin = ChunkPos::ORIGIN;
    let right = ChunkPos { x: 1, z: 0 };
    let mut chunks = BTreeMap::from([(origin, solid(origin, &[0])), (right, solid(right, &[0]))]);
    let job = MeshJob::capture(&chunks, origin, AIR).unwrap();
    Arc::make_mut(chunks.get_mut(&right).unwrap()).set_block(BlockPos::new(16, 0, 0), AIR);
    assert!(!job.build().is_current(&chunks));
    let fresh = MeshJob::capture(&chunks, origin, AIR).unwrap().build();
    assert!(fresh.is_current(&chunks));
    assert!(
        fresh
            .quads
            .iter()
            .any(|q| q.axis == 0 && q.positive && q.size == [1, 1])
    );
    chunks.remove(&right);
    assert!(!fresh.is_current(&chunks));
}

#[test]
fn samples_wire_ports_at_the_same_height_and_rebuilds_the_neighboring_chunk() {
    let wire = BlockStateId::from_registry_index(2);
    let repeater = BlockStateId::from_registry_index(3);
    let origin = ChunkPos::ORIGIN;
    let right = ChunkPos { x: 1, z: 0 };
    let mut center = Chunk::new(origin, AIR);
    let mut east = Chunk::new(right, AIR);
    center.set_block(BlockPos::new(15, 0, 5), wire);
    center.set_block(BlockPos::new(14, 0, 5), wire);
    center.set_block(BlockPos::new(15, 0, 4), wire);
    center.set_block(BlockPos::new(15, 1, 6), wire);
    east.set_block(BlockPos::new(16, 0, 5), repeater);
    let mut chunks = BTreeMap::from([(origin, Arc::new(center)), (right, Arc::new(east))]);
    let wires = BTreeSet::from([wire.registry_index()]);
    let ports = BTreeMap::from([
        (wire.registry_index(), 0b1111),
        (repeater.registry_index(), 0b1010),
    ]);
    let materials = MeshMaterials {
        passable: wires.clone(),
        wires,
        ports,
        ..Default::default()
    };
    let build = |chunks: &Chunks| {
        MeshJob::capture(chunks, origin, AIR)
            .unwrap()
            .build_with_materials(&materials)
    };
    let mask = |mesh: &rustcraft_scene::mesh::ChunkMesh| {
        mesh.connected
            .iter()
            .find(|surface| surface.origin == [15, 0, 5])
            .unwrap()
            .ports
    };
    let mesh = build(&chunks);
    assert_eq!(mask(&mesh), 0b1110);
    assert!(mesh.quads.iter().all(|quad| quad.block != wire));
    Arc::make_mut(chunks.get_mut(&right).unwrap()).set_block(BlockPos::new(16, 0, 5), AIR);
    assert!(!mesh.is_current(&chunks));
    assert_eq!(mask(&build(&chunks)), 0b1100);
}

#[test]
fn emits_a_riser_only_from_the_low_end_across_chunk_and_section_boundaries() {
    let wire = BlockStateId::from_registry_index(2);
    let origin = ChunkPos::ORIGIN;
    let right = ChunkPos { x: 1, z: 0 };
    let mut center = Chunk::new(origin, AIR);
    let mut east = Chunk::new(right, AIR);
    center.set_block(BlockPos::new(15, 15, 4), wire);
    east.set_block(BlockPos::new(16, 15, 4), ROCK);
    east.set_block(BlockPos::new(16, 16, 4), wire);
    let materials = MeshMaterials {
        wires: BTreeSet::from([wire.registry_index()]),
        ports: BTreeMap::from([(wire.registry_index(), 0b1111)]),
        ..Default::default()
    };
    let mut chunks = BTreeMap::from([(origin, Arc::new(center)), (right, Arc::new(east))]);
    let build = |chunks: &Chunks, position| {
        MeshJob::capture(chunks, position, AIR)
            .unwrap()
            .build_with_materials(&materials)
    };
    let low = build(&chunks, origin);
    assert_eq!(
        (low.connected[0].ports, low.connected[0].ascending),
        (0b0010, 0b0010)
    );
    let high = build(&chunks, right);
    assert_eq!(
        (high.connected[0].ports, high.connected[0].ascending),
        (0b1000, 0)
    );
    Arc::make_mut(chunks.get_mut(&origin).unwrap()).set_block(BlockPos::new(15, 16, 4), ROCK);
    assert!(!high.is_current(&chunks));
    assert_eq!(build(&chunks, origin).connected[0].ports, 0);
    assert_eq!(build(&chunks, right).connected[0].ports, 0);
}
