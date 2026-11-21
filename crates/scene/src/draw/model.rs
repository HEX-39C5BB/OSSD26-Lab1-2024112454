use rustcraft_world::{ControlMaterial, ControlMount, TerrainModel};

use super::{BlockTextures, Vertex, face_light};
use crate::assets::TextureAtlas;

#[derive(Clone, Copy)]
struct Face {
    atlas: [f32; 4],
    fallback: [f32; 3],
    uv: [f32; 4],
}

impl Face {
    fn crop(self, uv: [f32; 4]) -> Self {
        Self { uv, ..self }
    }
}

pub fn append_dropped_block(
    vertices: &mut Vec<Vertex>,
    position: [f64; 3],
    eye: [f64; 3],
    color: [f32; 3],
    textures: Option<&BlockTextures>,
) {
    let [x, y, z]: [f32; 3] = std::array::from_fn(|axis| (position[axis] - eye[axis]) as f32);
    let faces = std::array::from_fn(|index| {
        let atlas = textures.map_or([0.0; 4], |texture| {
            if let Some(directional) = texture.directional {
                directional[index]
            } else {
                match index {
                    2 => texture.bottom,
                    3 => texture.top,
                    _ => texture.sides,
                }
            }
        });
        Face {
            atlas,
            fallback: color,
            uv: [0.0, 0.0, 1.0, 1.0],
        }
    });
    cuboid(
        vertices,
        [x - 0.125, y, z - 0.125],
        [x + 0.125, y + 0.25, z + 0.125],
        faces,
    );
}

pub fn terrain_model(model: TerrainModel, atlas: Option<&TextureAtlas>) -> Vec<Vertex> {
    let face = |key, fallback| Face {
        atlas: atlas
            .and_then(|atlas| atlas.tile(&format!("rustcraft_java:{key}")))
            .unwrap_or_default(),
        fallback,
        uv: [0.0, 0.0, 1.0, 1.0],
    };
    let mut vertices = Vec::new();
    let control_face = |material| match material {
        ControlMaterial::Stone => face("block/stone", [0.53, 0.51, 0.50]),
        ControlMaterial::Oak => face("block/oak_planks", [0.67, 0.48, 0.29]),
        ControlMaterial::Gold => face("block/gold_block", [0.87, 0.68, 0.20]),
        ControlMaterial::Iron => face("block/iron_block", [0.74, 0.74, 0.73]),
    };
    let facing = match model {
        TerrainModel::Lever {
            mount,
            facing,
            powered,
        } => {
            let stone = face("block/cobblestone", [0.52, 0.51, 0.48]);
            cuboid(
                &mut vertices,
                [0.28, 0.0, 0.24],
                [0.72, 0.19, 0.76],
                [stone; 6],
            );
            let handle = face("block/lever", [0.68, 0.50, 0.29]).crop([
                7.0 / 16.0,
                6.0 / 16.0,
                9.0 / 16.0,
                1.0,
            ]);
            let start = vertices.len();
            cuboid(
                &mut vertices,
                [0.44, 0.13, 0.44],
                [0.56, 0.73, 0.56],
                [handle; 6],
            );
            let angle = std::f32::consts::FRAC_PI_4;
            let (sin, cos) = if powered { angle } else { -angle }.sin_cos();
            for vertex in &mut vertices[start..] {
                let y = vertex.pos[1] - 0.13;
                let z = vertex.pos[2] - 0.5;
                vertex.pos[1] = 0.13 + y * cos - z * sin;
                vertex.pos[2] = 0.5 + y * sin + z * cos;
            }
            for vertex in &mut vertices {
                let [x, y, z] = vertex.pos;
                vertex.pos = match mount {
                    ControlMount::Floor => [x, y, z],
                    ControlMount::Wall => [x, 1.0 - z, y],
                    ControlMount::Ceiling => [x, 1.0 - y, 1.0 - z],
                };
            }
            facing
        }
        TerrainModel::Button {
            mount,
            material,
            facing,
            pressed,
        } => {
            let depth = if pressed { 0.0625 } else { 0.125 };
            let (min, max) = match mount {
                ControlMount::Wall => ([0.3125, 0.375, 0.0], [0.6875, 0.625, depth]),
                ControlMount::Floor => ([0.3125, 0.0, 0.375], [0.6875, depth, 0.625]),
                ControlMount::Ceiling => ([0.3125, 1.0 - depth, 0.375], [0.6875, 1.0, 0.625]),
            };
            let surface = control_face(material).crop([0.3125, 0.375, 0.6875, 0.625]);
            cuboid(&mut vertices, min, max, [surface; 6]);
            facing
        }
        TerrainModel::PressurePlate { material, pressed } => {
            let height = if pressed { 0.03125 } else { 0.0625 };
            cuboid(
                &mut vertices,
                [0.0625, 0.0, 0.0625],
                [0.9375, height, 0.9375],
                [control_face(material); 6],
            );
            0
        }
        TerrainModel::Ejector { facing, dropper } => {
            let side = face("block/furnace_side", [0.53, 0.51, 0.49]);
            let cap = face("block/furnace_top", [0.60, 0.58, 0.55]);
            let surface = match (dropper, facing >= 4) {
                (false, false) => "block/dispenser_front",
                (false, true) => "block/dispenser_front_vertical",
                (true, false) => "block/dropper_front",
                (true, true) => "block/dropper_front_vertical",
            };
            let front = face(surface, [0.45, 0.43, 0.41]);
            cuboid(
                &mut vertices,
                [0.0; 3],
                [1.0; 3],
                [side, side, cap, cap, side, front],
            );
            facing
        }
        TerrainModel::RedstoneTorch { wall_facing, lit } => {
            let torch = face(
                if lit {
                    "block/redstone_torch"
                } else {
                    "block/redstone_torch_off"
                },
                if lit {
                    [1.0, 0.31, 0.12]
                } else {
                    [0.52, 0.33, 0.16]
                },
            );
            let side = torch.crop([7.0 / 16.0, 6.0 / 16.0, 9.0 / 16.0, 1.0]);
            let end = torch.crop([7.0 / 16.0, 6.0 / 16.0, 9.0 / 16.0, 8.0 / 16.0]);
            cuboid(
                &mut vertices,
                [0.4375, 0.0, 0.4375],
                [0.5625, 0.625, 0.5625],
                [side, side, end, end, side, side],
            );
            if wall_facing.is_some() {
                let (sin, cos) = (std::f32::consts::PI / 8.0).sin_cos();
                for vertex in &mut vertices {
                    let y = vertex.pos[1];
                    let z = vertex.pos[2] - 0.5;
                    vertex.pos[1] = 0.22 + y * cos - z * sin;
                    vertex.pos[2] = 0.08 + y * sin + z * cos;
                }
            }
            wall_facing.unwrap_or(0)
        }
        TerrainModel::Repeater {
            facing,
            powered,
            delay_rt,
            locked,
        } => {
            let side = face("smooth_stone", [0.68, 0.65, 0.60]);
            let top = face(
                if powered { "repeater_on" } else { "repeater" },
                [0.72, 0.62, 0.57],
            );
            let mut faces = [side.crop([0.0, 0.875, 1.0, 1.0]); 6];
            faces[2] = side;
            faces[3] = top;
            cuboid(&mut vertices, [0.0; 3], [1.0, 0.125, 1.0], faces);
            let torch = face(
                if powered {
                    "block/redstone_torch"
                } else {
                    "block/redstone_torch_off"
                },
                if powered {
                    [1.0, 0.22, 0.06]
                } else {
                    [0.46, 0.13, 0.08]
                },
            )
            .crop([7.0 / 16.0, 6.0 / 16.0, 9.0 / 16.0, 11.0 / 16.0]);
            let rear = 0.49 - f32::from(delay_rt - 1) * 0.125;
            if locked {
                let bar = face("bedrock", [0.28, 0.27, 0.26]);
                cuboid(
                    &mut vertices,
                    [0.125, 0.125, rear],
                    [0.875, 0.25, rear + 0.125],
                    [bar; 6],
                );
            }
            for z in [(!locked).then_some(rear), Some(0.77)]
                .into_iter()
                .flatten()
            {
                cuboid(
                    &mut vertices,
                    [0.44, 0.125, z],
                    [0.56, 0.46, z + 0.12],
                    [torch; 6],
                );
            }
            facing
        }
        TerrainModel::Comparator {
            facing,
            powered,
            subtract,
        } => {
            let side = face("smooth_stone", [0.68, 0.65, 0.60]);
            let top = face(
                if powered {
                    "block/comparator_on"
                } else {
                    "block/comparator"
                },
                [0.72, 0.62, 0.57],
            );
            let mut faces = [side.crop([0.0, 0.875, 1.0, 1.0]); 6];
            faces[3] = top;
            cuboid(&mut vertices, [0.0; 3], [1.0, 0.125, 1.0], faces);
            for (x, z, lit) in [
                (0.25, 0.26, powered),
                (0.75, 0.26, powered),
                (0.5, 0.75, subtract),
            ] {
                let torch = face(
                    if lit {
                        "block/redstone_torch"
                    } else {
                        "block/redstone_torch_off"
                    },
                    if lit {
                        [1.0, 0.22, 0.06]
                    } else {
                        [0.46, 0.13, 0.08]
                    },
                )
                .crop([7.0 / 16.0, 6.0 / 16.0, 9.0 / 16.0, 11.0 / 16.0]);
                cuboid(
                    &mut vertices,
                    [x - 0.06, 0.125, z - 0.06],
                    [x + 0.06, 0.44, z + 0.06],
                    [torch; 6],
                );
            }
            facing
        }
        TerrainModel::Cake { bites } => {
            let side = face("block/cake_side", [0.63, 0.45, 0.31]);
            let cut = if bites == 0 {
                side
            } else {
                face("block/cake_inner", [0.82, 0.61, 0.43])
            };
            let bottom = face("block/cake_bottom", [0.58, 0.38, 0.26]);
            let top = face("block/cake_top", [0.92, 0.83, 0.74]);
            cuboid(
                &mut vertices,
                [(1.0 + 2.0 * f32::from(bites)) / 16.0, 0.0, 0.0625],
                [0.9375, 0.5, 0.9375],
                [cut, side, bottom, top, side, side],
            );
            0
        }
        TerrainModel::Piston {
            facing,
            extended,
            sticky,
        } => {
            let side = face("piston", [0.59, 0.53, 0.41]);
            let bottom = face("block/piston_bottom", [0.46, 0.44, 0.40]);
            let front = face(
                if extended {
                    "block/piston_inner"
                } else if sticky {
                    "block/piston_top_sticky"
                } else {
                    "block/piston_top"
                },
                if sticky && !extended {
                    [0.44, 0.65, 0.27]
                } else {
                    [0.72, 0.57, 0.32]
                },
            );
            let depth = if extended { 0.75 } else { 1.0 };
            piston_cuboid(
                &mut vertices,
                [0.0; 3],
                [1.0, 1.0, depth],
                side,
                bottom,
                front,
            );
            facing
        }
        TerrainModel::PistonHead { facing, sticky } => {
            let side = face("piston", [0.59, 0.53, 0.41]);
            let top = if sticky {
                face("block/piston_top_sticky", [0.44, 0.65, 0.27])
            } else {
                face("block/piston_top", [0.72, 0.57, 0.32])
            };
            piston_cuboid(&mut vertices, [0.0, 0.0, 0.75], [1.0; 3], side, top, top);
            let rod = side.crop([0.37, 0.08, 0.63, 0.26]);
            cuboid(
                &mut vertices,
                [0.375, 0.375, -0.25],
                [0.625, 0.625, 0.75],
                [rod; 6],
            );
            facing
        }
    };
    for vertex in &mut vertices {
        let [x, y, z] = vertex.pos;
        vertex.pos = match facing {
            1 => [z, y, 1.0 - x],
            2 => [1.0 - x, y, 1.0 - z],
            3 => [1.0 - z, y, x],
            4 => [x, 1.0 - z, y],
            5 => [x, z, 1.0 - y],
            _ => [x, y, z],
        };
    }
    vertices
}

fn cuboid(vertices: &mut Vec<Vertex>, min: [f32; 3], max: [f32; 3], faces: [Face; 6]) {
    box_faces(vertices, min, max, faces, |axis, _, u, v| match axis {
        0 => [v, 1.0 - u],
        1 => [v, u],
        _ => [u, 1.0 - v],
    });
}

fn piston_cuboid(
    vertices: &mut Vec<Vertex>,
    min: [f32; 3],
    max: [f32; 3],
    side: Face,
    back: Face,
    front: Face,
) {
    box_faces(
        vertices,
        min,
        max,
        [side, side, side, side, back, front],
        |axis, pos, u, v| match axis {
            0 => [u, 1.0 - pos[2]],
            1 => [v, 1.0 - pos[2]],
            _ => [u, 1.0 - v],
        },
    );
}

fn box_faces(
    vertices: &mut Vec<Vertex>,
    min: [f32; 3],
    max: [f32; 3],
    faces: [Face; 6],
    uv_axis: impl Fn(usize, [f32; 3], f32, f32) -> [f32; 2],
) {
    for (index, face) in faces.into_iter().enumerate() {
        let axis = index / 2;
        let positive = index % 2 != 0;
        let u = (axis + 1) % 3;
        let v = (axis + 2) % 3;
        let light = face_light(axis, positive);
        let tint = if face.atlas[2] > 0.0 {
            [1.0; 3]
        } else {
            face.fallback
        };
        let color = tint.map(|channel| channel * light);
        let quad = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]].map(|[a, b]| {
            let mut pos = min;
            pos[axis] = if positive { max[axis] } else { min[axis] };
            pos[u] += a * (max[u] - min[u]);
            pos[v] += b * (max[v] - min[v]);
            let [x, y] = uv_axis(axis, pos, a, b);
            let [left, top, right, bottom] = face.uv;
            let uv = if face.atlas[2] > 0.0 {
                [
                    (left + x * (right - left)).min(0.9999),
                    (top + y * (bottom - top)).min(0.9999),
                ]
            } else {
                [-1.0; 2]
            };
            Vertex {
                pos,
                uv,
                color,
                atlas: face.atlas,
            }
        });
        vertices.extend([0, 1, 2, 0, 2, 3].map(|index| quad[index]));
    }
}
