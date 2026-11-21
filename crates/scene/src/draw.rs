mod debug;
mod inventory;
mod menu;
mod model;
mod outline;
mod text;

pub use debug::{HUD_ACCENT, HUD_TEXT, HudCorner, HudLine, colored_hud, command_hud};
pub use inventory::{InventoryHit, InventoryLayout};
pub use menu::{PAUSE_MENU_ROWS, PauseMenu, pause_menu};
pub use model::{append_dropped_block, terrain_model};
pub use outline::{
    append_block_outline, append_chunk_border, append_colored_block_outline, append_signal_arrow,
};

use std::collections::BTreeMap;
use std::f32::consts::PI;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{
    Vec3,
    camera::lh::{proj::directx, view},
};
use rustcraft_world::{BlockStateId, ChunkPos, HORIZONTAL_NEIGHBORS, SECTION_EDGE};

use crate::assets::TextureAtlas;
use crate::mesh::ChunkMesh;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 3],
    pub uv: [f32; 2],
    pub atlas: [f32; 4],
}

pub struct Frame {
    pub terrain: Vec<Vertex>,
    pub hud: Vec<Vertex>,
    pub view_projection: [[f32; 4]; 4],
    pub atlas: Option<Arc<TextureAtlas>>,
    pub background: [f32; 4],
}

pub const NATIVE_SKY: [f32; 4] = [0.15, 0.20, 0.28, 1.0];
pub const FLAT_SKY: [f32; 4] = [0.46, 0.68, 0.96, 1.0];

pub type Colors = BTreeMap<usize, [f32; 3]>;

#[derive(Clone, Copy)]
pub struct BlockTextures {
    pub top: [f32; 4],
    pub sides: [f32; 4],
    pub bottom: [f32; 4],
    pub directional: Option<[[f32; 4]; 6]>,
    pub top_tint: [f32; 3],
    pub side_overlay: Option<[f32; 4]>,
    pub wire_line: Option<[f32; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Crosshair {
    Idle,
    Targeted,
    Accepted,
    Rejected,
}

const FALLBACK: [f32; 3] = [0.94, 0.16, 0.69];
const VERTICAL_FOV: f32 = PI * 0.39;
const FAR_PLANE: f32 = 512.0;

pub fn material(colors: &Colors, block: BlockStateId) -> [f32; 3] {
    colors
        .get(&block.registry_index())
        .copied()
        .unwrap_or(FALLBACK)
}

pub fn perspective(yaw: f64, pitch: f64, aspect: f32) -> [[f32; 4]; 4] {
    let (sin_yaw, cos_yaw) = (yaw as f32).sin_cos();
    let (sin_pitch, cos_pitch) = (pitch as f32).sin_cos();
    let forward = Vec3::new(sin_yaw * cos_pitch, sin_pitch, cos_yaw * cos_pitch);
    let right = Vec3::new(cos_yaw, 0.0, -sin_yaw);
    let up = forward.cross(right);
    let view = view::look_to_mat4(Vec3::ZERO, forward, up);
    let mut projection = directx::perspective(VERTICAL_FOV, aspect, 0.045, FAR_PLANE);
    projection.y_axis.y *= -1.0;
    (projection * view).to_cols_array_2d()
}

pub fn append_terrain(
    vertices: &mut Vec<Vertex>,
    mesh: &ChunkMesh,
    colors: &Colors,
    textures: &BTreeMap<usize, BlockTextures>,
    models: &BTreeMap<usize, Vec<Vertex>>,
    eye: [f64; 3],
) {
    let ChunkPos { x, z } = mesh.position();
    let extent = SECTION_EDGE as f64;
    let offset = [
        x as f64 * extent - eye[0],
        -eye[1],
        z as f64 * extent - eye[2],
    ];
    for quad in &mesh.quads {
        let u = (quad.axis + 1) % 3;
        let v = (quad.axis + 2) % 3;
        let [width, height] = quad.size.map(f32::from);
        let base = std::array::from_fn(|i| (quad.origin[i] as f64 + offset[i]) as f32);
        let light = face_light(quad.axis, quad.positive);
        let texture = textures.get(&quad.block.registry_index());
        let (atlas, tint) = match texture {
            Some(BlockTextures {
                directional: Some(faces),
                ..
            }) => (faces[2 * quad.axis + usize::from(quad.positive)], [1.0; 3]),
            Some(texture) => match (quad.axis, quad.positive) {
                (1, true) => (texture.top, texture.top_tint),
                (1, false) => (texture.bottom, [1.0; 3]),
                _ => (texture.sides, [1.0; 3]),
            },
            None => ([0.0; 4], material(colors, quad.block)),
        };
        let color = tint.map(|channel| channel * light);
        let quad_vertices = [[0.0, 0.0], [width, 0.0], [width, height], [0.0, height]].map(|uv| {
            let mut pos = base;
            pos[u] += uv[0];
            pos[v] += uv[1];
            let uv = if atlas[2] > 0.0 {
                match quad.axis {
                    0 => [uv[1], width - uv[0]],
                    1 => [uv[1], uv[0]],
                    _ => [uv[0], height - uv[1]],
                }
            } else {
                uv
            };
            Vertex {
                pos,
                color,
                uv,
                atlas,
            }
        });
        let indices = if quad.positive {
            [0, 1, 2, 0, 2, 3]
        } else {
            [0, 2, 1, 0, 3, 2]
        };
        vertices.extend(indices.map(|index| quad_vertices[index]));
        if quad.axis != 1
            && let Some((texture, overlay)) =
                texture.and_then(|texture| Some((texture, texture.side_overlay?)))
        {
            vertices.extend(indices.map(|index| {
                let mut vertex = quad_vertices[index];
                vertex.pos[quad.axis] += if quad.positive { 0.002 } else { -0.002 };
                vertex.color = texture.top_tint.map(|channel| channel * light);
                vertex.atlas = overlay;
                vertex
            }));
        }
    }
    for (origin, block) in &mesh.models {
        let origin: [f32; 3] =
            std::array::from_fn(|axis| (origin[axis] as f64 + offset[axis]) as f32);
        if let Some(model) = models.get(&block.registry_index()) {
            vertices.extend(model.iter().map(|vertex| Vertex {
                pos: std::array::from_fn(|axis| vertex.pos[axis] + origin[axis]),
                ..*vertex
            }));
        }
    }
    for surface in &mesh.connected {
        let origin: [f32; 3] =
            std::array::from_fn(|axis| (surface.origin[axis] as f64 + offset[axis]) as f32);
        let texture = textures.get(&surface.block.registry_index());
        let color = texture.map_or_else(
            || material(colors, surface.block),
            |texture| texture.top_tint,
        );
        let dot = texture.map_or([0.0; 4], |texture| texture.top);
        let line = texture
            .and_then(|texture| texture.wire_line)
            .unwrap_or([0.0; 4]);
        let thin = 0.12;
        if !matches!(surface.ports, 0b0101 | 0b1010) {
            let rect = if dot[2] > 0.0 {
                [0.0, 0.0, 1.0, 1.0]
            } else {
                [0.32, 0.32, 0.68, 0.68]
            };
            surface_quad(
                vertices,
                [origin[0], origin[1] + 0.021, origin[2]],
                rect,
                dot,
                color,
                false,
            );
        }
        for direction in 0..4 {
            if surface.ports & (1 << direction) == 0 {
                continue;
            }
            let (low, high) = if line[2] > 0.0 {
                (0.0, 1.0)
            } else {
                (0.5 - thin, 0.5 + thin)
            };
            let rect = match direction {
                0 => [low, 0.5, high, 1.0],
                1 => [0.5, low, 1.0, high],
                2 => [low, 0.0, high, 0.5],
                _ => [0.0, low, 0.5, high],
            };
            surface_quad(
                vertices,
                [origin[0], origin[1] + 0.022, origin[2]],
                rect,
                line,
                color,
                direction % 2 != 0,
            );
            if surface.ascending & (1 << direction) != 0 {
                wire_riser(vertices, origin, direction, [low, high], line, color);
            }
        }
    }
}

fn face_light(axis: usize, positive: bool) -> f32 {
    match (axis, positive) {
        (1, true) => 1.0,
        (1, false) => 0.63,
        (0, true) => 0.89,
        (0, false) => 0.78,
        (2, true) => 0.95,
        _ => 0.83,
    }
}

fn wire_riser(
    vertices: &mut Vec<Vertex>,
    origin: [f32; 3],
    direction: usize,
    [left, right]: [f32; 2],
    atlas: [f32; 4],
    color: [f32; 3],
) {
    let [dx, _, dz] = HORIZONTAL_NEIGHBORS[direction];
    let edge = if dx + dz > 0 { 0.998 } else { 0.002 };
    let quad = [[left, 0.0], [right, 0.0], [right, 1.0], [left, 1.0]].map(|[across, height]| {
        let (x, z) = if dx == 0 {
            (across, edge)
        } else {
            (edge, across)
        };
        Vertex {
            pos: [origin[0] + x, origin[1] + 0.022 + height, origin[2] + z],
            color,
            uv: if atlas[2] > 0.0 {
                [across, 1.0 - height]
            } else {
                [-1.0; 2]
            },
            atlas,
        }
    });
    vertices.extend([0, 1, 2, 0, 2, 3].map(|index| quad[index]));
}

fn surface_quad(
    vertices: &mut Vec<Vertex>,
    origin: [f32; 3],
    [left, top, right, bottom]: [f32; 4],
    atlas: [f32; 4],
    color: [f32; 3],
    rotate: bool,
) {
    let quad = [[left, top], [right, top], [right, bottom], [left, bottom]].map(|[x, z]| Vertex {
        pos: [origin[0] + x, origin[1], origin[2] + z],
        color,
        uv: if atlas[2] <= 0.0 {
            [0.0, -1.0]
        } else if rotate {
            [z, x]
        } else {
            [x, z]
        },
        atlas,
    });
    vertices.extend([0, 1, 2, 0, 2, 3].map(|index| quad[index]));
}

pub fn append_sky(vertices: &mut Vec<Vertex>, atlas: &TextureAtlas, eye: [f64; 3], tick: u64) {
    let append = |vertices: &mut Vec<Vertex>, pos: [[f32; 3]; 4], uv: [[f32; 2]; 4], name: &str| {
        if let Some(atlas) = atlas.tile(name) {
            vertices.extend([0, 1, 2, 0, 2, 3].map(|index| Vertex {
                pos: pos[index],
                color: [1.0; 3],
                uv: uv[index],
                atlas,
            }));
        }
    };
    let height = (120.0 - eye[1]) as f32;
    let radius = 370.0;
    let width = 180.0;
    let ox = ((eye[0] - f64::from(radius)) / width + tick as f64 * 0.00005).rem_euclid(1.0) as f32;
    let oz = ((eye[2] - f64::from(radius)) / width).rem_euclid(1.0) as f32;
    let span = 2.0 * radius / width as f32;
    append(
        vertices,
        [
            [-radius, height, -radius],
            [radius, height, -radius],
            [radius, height, radius],
            [-radius, height, radius],
        ],
        [
            [ox, oz],
            [ox + span, oz],
            [ox + span, oz + span],
            [ox, oz + span],
        ],
        "rustcraft_java:environment/clouds",
    );
    append(
        vertices,
        [
            [-19.0, 165.0, 300.0],
            [19.0, 165.0, 300.0],
            [19.0, 127.0, 300.0],
            [-19.0, 127.0, 300.0],
        ],
        [[0.0, 0.0], [0.9999, 0.0], [0.9999, 0.9999], [0.0, 0.9999]],
        "rustcraft_java:environment/sun",
    );
}

/// Presentation-only item appearance. Texture coordinates refer to the frame atlas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ItemIcon {
    Solid([f32; 3]),
    Texture([f32; 4]),
}

pub fn hotbar(
    [width, height]: [u32; 2],
    slots: &[Option<(ItemIcon, u16)>],
    selected: usize,
    crosshair: Crosshair,
) -> Vec<Vertex> {
    let [w, h] = [width as f32, height as f32];
    let mut panel = Panel {
        vertices: Vec::new(),
        size: [w, h],
    };
    let crosshair = match crosshair {
        Crosshair::Idle => [0.94; 3],
        Crosshair::Targeted => [1.0, 0.97, 0.70],
        Crosshair::Accepted => [0.35, 1.0, 0.74],
        Crosshair::Rejected => [1.0, 0.31, 0.32],
    };
    panel.rect([w / 2.0 - 7.0, h / 2.0 - 1.0, 14.0, 2.0], crosshair);
    panel.rect([w / 2.0 - 1.0, h / 2.0 - 7.0, 2.0, 14.0], crosshair);

    let edge = (w / 11.0).clamp(22.0, 55.0);
    let stride = edge + 4.0;
    let left = (w - stride * slots.len() as f32 + 4.0) / 2.0;
    let top = h - edge - 14.0;
    for (index, slot) in slots.iter().enumerate() {
        let x = left + index as f32 * stride;
        let outline = if index == selected {
            [1.0, 0.87, 0.46]
        } else {
            [0.27, 0.32, 0.37]
        };
        panel.rect([x, top, edge, edge], outline);
        panel.rect(
            [x + 2.0, top + 2.0, edge - 4.0, edge - 4.0],
            [0.075, 0.095, 0.13],
        );
        if let Some((icon, count)) = slot {
            panel.item([x + 7.0, top + 7.0, edge - 14.0, edge - 14.0], *icon);
            let scale = if edge >= 40.0 { 2.0 } else { 1.0 };
            let mut cursor = x + edge - 5.0;
            for digit in count.to_string().bytes().rev() {
                cursor -= 4.0 * scale;
                panel.digit(digit - b'0', cursor, top + edge - 4.0 - 5.0 * scale, scale);
            }
        }
    }
    panel.vertices
}

struct Panel {
    vertices: Vec<Vertex>,
    size: [f32; 2],
}

impl Panel {
    fn item(&mut self, rect: [f32; 4], icon: ItemIcon) {
        match icon {
            ItemIcon::Solid(color) => self.rect(rect, color),
            ItemIcon::Texture(atlas) => {
                let start = self.vertices.len();
                self.rect(rect, [1.0; 3]);
                // The vertex shader treats a negative x UV as the HUD-space
                // marker. Keep that sign while preserving [0, 1) texture
                // coordinates through fract() in the fragment shader.
                let uv = [
                    [-1.0, 0.0],
                    [-0.0001, 0.0],
                    [-0.0001, 0.9999],
                    [-1.0, 0.9999],
                ];
                for (vertex, index) in self.vertices[start..].iter_mut().zip([0, 1, 2, 0, 2, 3]) {
                    vertex.uv = uv[index];
                    vertex.atlas = atlas;
                }
            }
        }
    }

    fn rect(&mut self, [x, y, w, h]: [f32; 4], color: [f32; 3]) {
        let corners = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]].map(|[x, y]| Vertex {
            pos: [
                2.0 * x / self.size[0] - 1.0,
                2.0 * y / self.size[1] - 1.0,
                0.0,
            ],
            color,
            uv: [-1.0; 2],
            atlas: [0.0; 4],
        });
        self.vertices.extend([0, 1, 2, 0, 2, 3].map(|i| corners[i]));
    }

    fn digit(&mut self, digit: u8, x: f32, y: f32, scale: f32) {
        const DIGITS: [[u8; 5]; 10] = [
            [7, 5, 5, 5, 7],
            [2, 6, 2, 2, 7],
            [7, 1, 7, 4, 7],
            [7, 1, 7, 1, 7],
            [5, 5, 7, 1, 1],
            [7, 4, 7, 1, 7],
            [7, 4, 7, 5, 7],
            [7, 1, 1, 1, 1],
            [7, 5, 7, 5, 7],
            [7, 5, 7, 1, 7],
        ];
        self.rect(
            [x - 0.5 * scale, y - 0.5 * scale, 4.0 * scale, 6.0 * scale],
            [0.035, 0.045, 0.06],
        );
        for (row, bits) in DIGITS[usize::from(digit)].iter().enumerate() {
            for col in 0..3 {
                if bits & (4 >> col) != 0 {
                    self.rect(
                        [x + col as f32 * scale, y + row as f32 * scale, scale, scale],
                        [0.99; 3],
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_item_icons_remain_flagged_as_screen_space() {
        let vertices = hotbar(
            [960, 600],
            &[Some((ItemIcon::Texture([16.0, 32.0, 64.0, 64.0]), 1))],
            0,
            Crosshair::Idle,
        );
        let icons: Vec<_> = vertices
            .iter()
            .filter(|vertex| vertex.atlas[2] > 0.0)
            .collect();
        assert_eq!(icons.len(), 6);
        assert!(
            icons.iter().all(|vertex| vertex.uv[0] < 0.0),
            "HUD item icons must keep a negative x UV so the vertex shader uses screen space"
        );
        assert!(icons.iter().all(|vertex| vertex.uv[0] >= -1.0));
        assert!(
            icons
                .iter()
                .all(|vertex| (0.0..=0.9999).contains(&vertex.uv[1]))
        );
    }
}
