use rustcraft_world::{BlockPos, SECTION_EDGE, VerticalRange};

use super::{FAR_PLANE, VERTICAL_FOV, Vertex};

const WIDTH: f32 = 0.026;
const SURFACE_OFFSET: f32 = 0.004;
const COLOR: [f32; 3] = [1.0, 0.90, 0.47];
const CORNER_COLOR: [f32; 3] = [1.0, 0.90, 0.14];
const LAYER_COLOR: [f32; 3] = [0.22, 0.92, 0.27];
const SECTION_COLOR: [f32; 3] = [0.23, 0.95, 0.88];

/// Draw a border on the faces of the selected voxel. World depth occludes back edges.
pub fn append_block_outline(vertices: &mut Vec<Vertex>, block: BlockPos, eye: [f64; 3]) {
    append_colored_block_outline(vertices, block, eye, COLOR);
}

pub fn append_colored_block_outline(
    vertices: &mut Vec<Vertex>,
    block: BlockPos,
    eye: [f64; 3],
    color: [f32; 3],
) {
    let [x, y, z] = [block.x, block.y, block.z];
    let origin = [
        (x as f64 - eye[0]) as f32,
        (y as f64 - eye[1]) as f32,
        (z as f64 - eye[2]) as f32,
    ];
    for axis in 0..3 {
        let u = (axis + 1) % 3;
        let v = (axis + 2) % 3;
        for face in [-SURFACE_OFFSET, 1.0 + SURFACE_OFFSET] {
            for [left, top, width, height] in [
                [0.0, 0.0, 1.0, WIDTH],
                [0.0, 1.0 - WIDTH, 1.0, WIDTH],
                [0.0, WIDTH, WIDTH, 1.0 - 2.0 * WIDTH],
                [1.0 - WIDTH, WIDTH, WIDTH, 1.0 - 2.0 * WIDTH],
            ] {
                let corners = [
                    [left, top],
                    [left + width, top],
                    [left + width, top + height],
                    [left, top + height],
                ]
                .map(|[du, dv]| {
                    let mut pos = origin;
                    pos[axis] += face;
                    pos[u] += du;
                    pos[v] += dv;
                    Vertex {
                        pos,
                        color,
                        uv: [0.0, -1.0],
                        atlas: [0.0; 4],
                    }
                });
                vertices.extend([0, 1, 2, 0, 2, 3].map(|index| corners[index]));
            }
        }
    }
}

/// Draw the posts of the surrounding 3×3 chunks and nearby layer rings in the current chunk.
pub fn append_chunk_border(
    vertices: &mut Vec<Vertex>,
    player: BlockPos,
    height: VerticalRange,
    eye: [f64; 3],
) {
    // Bound geometry for unusually tall worlds by the far plane's maximum vertical reach.
    let reach = (FAR_PLANE / (VERTICAL_FOV / 2.0).cos()).ceil() as i64 + 1;
    let eye_level = eye[1].floor() as i64;
    let bottom = height.min.max(eye_level.saturating_sub(reach));
    let top = height.max_exclusive.min(eye_level.saturating_add(reach));
    if bottom >= top {
        return;
    }

    let side = SECTION_EDGE as i64;
    let extent = side as f32;
    let [x, z] = [(player.x, eye[0]), (player.z, eye[2])]
        .map(|(position, camera)| ((position.div_euclid(side) * side) as f64 - camera) as f32);
    let bottom_y = (bottom as f64 - eye[1]) as f32;
    let post_height = (top - bottom) as f32;
    for dx in -1..=2 {
        for dz in -1..=2 {
            chunk_line(
                vertices,
                [x + dx as f32 * extent, bottom_y, z + dz as f32 * extent],
                1,
                post_height,
                CORNER_COLOR,
                0.032,
            );
        }
    }

    let section_bottom = player.y.div_euclid(side) * side;
    let ring_bottom = bottom.max(section_bottom.saturating_sub(3 * side));
    let ring_top = top.min(section_bottom.saturating_add(4 * side));
    for level in ring_bottom..=ring_top {
        let (color, half_width) = if level.rem_euclid(side) == 0 {
            (SECTION_COLOR, 0.032)
        } else {
            (LAYER_COLOR, 0.016)
        };
        let y = (level as f64 - eye[1]) as f32 + 0.033;
        for offset in [0.0, extent] {
            chunk_line(vertices, [x, y, z + offset], 0, extent, color, half_width);
            chunk_line(vertices, [x + offset, y, z], 2, extent, color, half_width);
        }
    }
}

fn chunk_line(
    vertices: &mut Vec<Vertex>,
    start: [f32; 3],
    axis: usize,
    length: f32,
    color: [f32; 3],
    half_width: f32,
) {
    for cross in [(axis + 1) % 3, (axis + 2) % 3] {
        let corners = [
            [0.0, -half_width],
            [length, -half_width],
            [length, half_width],
            [0.0, half_width],
        ]
        .map(|[along, across]| {
            let mut pos = start;
            pos[axis] += along;
            pos[cross] += across;
            Vertex {
                pos,
                color,
                // Negative U selects HUD projection. Negative V keeps world lines solid.
                uv: [0.0, -1.0],
                atlas: [0.0; 4],
            }
        });
        vertices.extend([0, 1, 2, 0, 2, 3].map(|index| corners[index]));
    }
}

pub fn append_signal_arrow(
    vertices: &mut Vec<Vertex>,
    source: BlockPos,
    target: BlockPos,
    eye: [f64; 3],
    color: [f32; 3],
) {
    use glam::Vec3;
    let center = |p: BlockPos| {
        Vec3::new(
            (p.x as f64 + 0.5 - eye[0]) as f32,
            (p.y as f64 + 1.09 - eye[1]) as f32,
            (p.z as f64 + 0.5 - eye[2]) as f32,
        )
    };
    let start = center(source);
    let end = center(target);
    let Some(direction) = (end - start).try_normalize() else {
        return;
    };
    let tangent = if direction.y.abs() > 0.9 {
        Vec3::X
    } else {
        direction.cross(Vec3::Y).normalize()
    };
    for side in [tangent, tangent.cross(direction)] {
        let tip = end - direction * 0.10;
        let base = tip - direction * 0.23;
        let start = start + direction * 0.08;
        let width = side * 0.026;
        let fin = side * 0.10;
        vertices.extend(
            [
                start - width,
                start + width,
                base + width,
                start - width,
                base + width,
                base - width,
                base - fin,
                base + fin,
                tip,
            ]
            .map(|pos| Vertex {
                pos: pos.to_array(),
                color,
                uv: [0.0, -1.0],
                atlas: [0.0; 4],
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surrounds_a_voxel_with_camera_relative_faces() {
        let mut vertices = Vec::new();
        append_block_outline(&mut vertices, BlockPos::new(-2, 6, 3), [-1.5, 6.5, 3.5]);
        assert_eq!(vertices.len(), 144);
        for axis in 0..3 {
            assert!(vertices.iter().any(|vertex| vertex.pos[axis] < -0.5));
            assert!(vertices.iter().any(|vertex| vertex.pos[axis] > 0.5));
        }
    }

    #[test]
    fn draws_nearby_layers_around_the_player_section_and_full_height_posts() {
        let mut vertices = Vec::new();
        let height = VerticalRange::FLAT_DEFAULT;
        let eye = [-0.5, -15.38, -0.5];
        append_chunk_border(&mut vertices, BlockPos::new(-1, -17, -1), height, eye);
        assert!(vertices.iter().all(|vertex| vertex.uv == [0.0, -1.0]));
        let (posts, rings) = vertices.split_at(16 * 12);
        for axis in [0, 2] {
            let coords = rings.iter().map(|vertex| vertex.pos[axis]);
            assert!(
                coords
                    .clone()
                    .all(|coordinate| (-15.54..=0.54).contains(&coordinate))
            );
            assert!(coords.clone().any(|coordinate| coordinate < -15.46));
            assert!(coords.clone().any(|coordinate| coordinate > 0.46));
        }
        let [bottom, top] =
            [height.min, height.max_exclusive].map(|level| (level as f64 - eye[1]) as f32);
        assert!(posts.iter().all(|vertex| vertex.color == CORNER_COLOR));
        assert!(
            posts
                .iter()
                .any(|vertex| (vertex.pos[1] - bottom).abs() < 0.001)
        );
        assert!(
            posts
                .iter()
                .any(|vertex| (vertex.pos[1] - top).abs() < 0.001)
        );
        let levels = height.min..=32;
        let (rings, remainder) = rings.as_chunks::<48>();
        assert!(remainder.is_empty());
        assert_eq!(rings.len(), levels.clone().count());
        for (level, ring) in levels.zip(rings) {
            let y = (level as f64 - eye[1]) as f32;
            let color = if level.rem_euclid(16) == 0 {
                [0.23, 0.95, 0.88]
            } else {
                LAYER_COLOR
            };
            assert!(ring.iter().all(|vertex| vertex.color == color));
            assert!(ring.iter().all(|vertex| (vertex.pos[1] - y).abs() < 0.066));
        }
    }

    #[test]
    fn draws_one_post_at_each_corner_of_the_surrounding_nine_chunks() {
        let mut vertices = Vec::new();
        append_chunk_border(
            &mut vertices,
            BlockPos::new(0, 0, 0),
            VerticalRange {
                min: -1,
                max_exclusive: 1,
            },
            [0.0; 3],
        );
        let (posts, rings) = vertices.split_at(16 * 12);
        let centers = [-16.0, 0.0, 16.0, 32.0];
        let expected = centers.into_iter().flat_map(|x| centers.map(|z| (x, z)));
        for (post, (x, z)) in posts.as_chunks::<12>().0.iter().zip(expected) {
            assert!(post.iter().all(|vertex| {
                vertex.color == CORNER_COLOR
                    && (vertex.pos[0] - x).abs() < 0.033
                    && (vertex.pos[2] - z).abs() < 0.033
            }));
        }
        assert_eq!(rings.len(), 3 * 48);
        assert!(rings.iter().all(|vertex| {
            vertex.color != CORNER_COLOR
                && [0, 2]
                    .into_iter()
                    .all(|axis| (-0.032..=16.032).contains(&vertex.pos[axis]))
        }));
    }

    #[test]
    fn bounds_geometry_to_the_frustum_in_extremely_tall_worlds() {
        let mut vertices = Vec::new();
        append_chunk_border(
            &mut vertices,
            BlockPos::new(0, 0, 0),
            VerticalRange {
                min: i64::MIN,
                max_exclusive: i64::MAX,
            },
            [0.0; 3],
        );
        assert_eq!(vertices.len(), 16 * 12 + (7 * 16 + 1) * 48);
        assert!(
            vertices
                .iter()
                .all(|vertex| vertex.pos[1].abs() < 2.0 * FAR_PLANE)
        );
    }
}
