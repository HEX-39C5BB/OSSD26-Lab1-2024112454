use std::f64::consts::{FRAC_PI_2, TAU};

use bevy_ecs::prelude::Component;
use rustcraft_world::{BlockPos, ChunkPos};

use crate::checkpoint::PlayerMotion;
use crate::dimension::Dimension;
use crate::{EntityId, RuleVersion};

const HALF_WIDTH: f64 = 0.30;
const HEIGHT: f64 = 1.80;
const EYE_HEIGHT: f64 = 1.62;
const WALK_STEP: f64 = 0.24;
const JUMP_SPEED: f64 = 0.40;
const FLIGHT_STEP: f64 = 0.30;
const DOUBLE_JUMP_TICKS: u64 = 7;
const GRAVITY: f64 = 0.06;
const MAX_FALL_SPEED: f64 = 0.9;
const BLOCK_REACH_V1: f64 = 5.0;
const CONTACT_EPSILON: f64 = 1e-9;

#[derive(Clone, Copy)]
pub(crate) enum PlayerMovement {
    NativeV1,
}

impl PlayerMovement {
    pub(crate) const fn identity(self) -> RuleVersion {
        match self {
            Self::NativeV1 => RuleVersion {
                id: "rustcraft:player_movement",
                version: 1,
            },
        }
    }

    pub(crate) fn restore(self, motion: PlayerMotion) -> Option<Player> {
        match self {
            Self::NativeV1 => Player::restore_v1(motion),
        }
    }

    pub(crate) fn set_input(self, player: &mut Player, input: PlayerInput, tick: u64) {
        match self {
            Self::NativeV1 => player.set_input_v1(input, tick),
        }
    }

    /// Returns the actual grounded distance traveled this tick.
    pub(crate) fn step(self, player: &mut Player, dimension: &Dimension) -> f64 {
        match self {
            Self::NativeV1 => player.step_v1(dimension),
        }
    }
}

/// A first-person input sample. Yaw zero faces +Z, and positive pitch looks up.
/// Horizontal input axes are -1, 0, or 1. Angles are radians.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerInput {
    pub forward: i8,
    pub strafe: i8,
    pub yaw: f64,
    pub pitch: f64,
    pub jump: bool,
    pub descend: bool,
    /// Number of space presses since the previous sample, including taps that were released.
    pub jump_presses: u8,
}

impl PlayerInput {
    pub(crate) fn validated(mut self) -> Option<Self> {
        if !(-1..=1).contains(&self.forward)
            || !(-1..=1).contains(&self.strafe)
            || self.jump_presses > 2
            || !self.yaw.is_finite()
            || !(-FRAC_PI_2..=FRAC_PI_2).contains(&self.pitch)
        {
            return None;
        }
        self.yaw = self.yaw.rem_euclid(TAU);
        Some(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockAction {
    Break,
    Place,
    Pick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockTarget {
    pub position: BlockPos,
    pub revision: u64,
    pub placement: Option<BlockPlacement>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockPlacement {
    pub position: BlockPos,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerSnapshot {
    pub entity: EntityId,
    /// Feet position [x, y, z] in block units.
    pub position: [f64; 3],
    pub yaw: f64,
    pub pitch: f64,
    pub grounded: bool,
    pub flying: bool,
    pub target: Option<BlockTarget>,
}

impl PlayerSnapshot {
    pub fn block_position(self) -> BlockPos {
        block_position(self.position)
    }

    pub fn eye_position(&self) -> [f64; 3] {
        let [x, y, z] = self.position;
        [x, y + EYE_HEIGHT, z]
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct Player {
    position: [f64; 3],
    vertical_speed: f64,
    grounded: bool,
    flying: bool,
    input: PlayerInput,
    jump_requested: bool,
    last_jump_press: Option<u64>,
}

impl Player {
    pub(crate) fn new(y: f64) -> Self {
        Self {
            position: [0.5, y, 0.5],
            vertical_speed: 0.0,
            grounded: false,
            flying: false,
            input: PlayerInput::default(),
            jump_requested: false,
            last_jump_press: None,
        }
    }

    pub(crate) fn block_position(self) -> BlockPos {
        block_position(self.position)
    }

    pub(crate) fn motion(self) -> PlayerMotion {
        PlayerMotion {
            position: self.position,
            yaw: self.input.yaw,
            pitch: self.input.pitch,
            vertical_speed: self.vertical_speed,
            grounded: self.grounded,
            flying: self.flying,
        }
    }

    fn restore_v1(motion: PlayerMotion) -> Option<Self> {
        let input = PlayerInput {
            yaw: motion.yaw,
            pitch: motion.pitch,
            ..PlayerInput::default()
        };
        input.validated()?;
        if !(0.0..TAU).contains(&motion.yaw) && motion.yaw != TAU {
            return None;
        }
        if !motion
            .position
            .iter()
            .all(|value| value.is_finite() && value.abs() < (1_u64 << 50) as f64)
            || !(-MAX_FALL_SPEED..=JUMP_SPEED).contains(&motion.vertical_speed)
            || (motion.flying && (motion.grounded || motion.vertical_speed != 0.0))
        {
            return None;
        }
        Some(Self {
            position: motion.position,
            vertical_speed: motion.vertical_speed,
            grounded: motion.grounded,
            flying: motion.flying,
            input,
            jump_requested: false,
            last_jump_press: None,
        })
    }

    pub(crate) fn has_space(self, dimension: &Dimension) -> bool {
        let [mut x, y, z] = self.bounds().block_ranges();
        x.all(|x| {
            y.clone().all(|y| {
                z.clone().all(|z| {
                    dimension
                        .voxel(BlockPos::new(x, y, z))
                        .is_some_and(|(state, _)| dimension.is_passable(state))
                })
            })
        })
    }

    pub(crate) fn teleport(&mut self, position: [f64; 3]) {
        self.position = position;
        self.vertical_speed = 0.0;
        self.grounded = false;
        self.clear_input();
    }

    pub(crate) fn look(&mut self, yaw: f64, pitch: f64) {
        self.input.yaw = yaw.rem_euclid(TAU);
        self.input.pitch = pitch;
    }

    pub(crate) fn occupied_chunks(self) -> impl Iterator<Item = ChunkPos> {
        let [x, _, z] = self.bounds().block_ranges();
        let edge = rustcraft_world::SECTION_EDGE as i64;
        let first_x = x.start.div_euclid(edge);
        let last_x = (x.end - 1).div_euclid(edge);
        let first_z = z.start.div_euclid(edge);
        let last_z = (z.end - 1).div_euclid(edge);
        (first_x..=last_x).flat_map(move |x| (first_z..=last_z).map(move |z| ChunkPos { x, z }))
    }

    pub(crate) fn sneaking(self) -> bool {
        self.input.descend && !self.flying
    }

    fn set_input_v1(&mut self, mut input: PlayerInput, tick: u64) {
        let presses =
            std::mem::take(&mut input.jump_presses).max(u8::from(input.jump && !self.input.jump));
        for _ in 0..presses {
            if self
                .last_jump_press
                .is_some_and(|last| tick - last <= DOUBLE_JUMP_TICKS)
            {
                self.flying = !self.flying;
                self.grounded = false;
                self.vertical_speed = 0.0;
                self.last_jump_press = None;
                self.jump_requested = self.flying;
            } else {
                self.last_jump_press = Some(tick);
                self.jump_requested |= !self.flying;
            }
        }
        self.input = input;
    }

    pub(crate) fn clear_input(&mut self) {
        self.input.forward = 0;
        self.input.strafe = 0;
        self.input.jump = false;
        self.input.descend = false;
        self.jump_requested = false;
        self.last_jump_press = None;
    }

    fn step_v1(&mut self, dimension: &Dimension) -> f64 {
        let [x, _, z] = self.position;
        let started_grounded = self.grounded;
        let (sin_yaw, cos_yaw) = self.input.yaw.sin_cos();
        let forward = f64::from(self.input.forward);
        let strafe = f64::from(self.input.strafe);
        let scale = WALK_STEP / forward.hypot(strafe).max(1.0);
        self.slide(dimension, 0, (sin_yaw * forward + cos_yaw * strafe) * scale);
        self.slide(dimension, 2, (cos_yaw * forward - sin_yaw * strafe) * scale);

        let jump_requested = std::mem::take(&mut self.jump_requested);
        if self.flying {
            let vertical = if jump_requested {
                1
            } else {
                i8::from(self.input.jump) - i8::from(self.input.descend)
            };
            let contact = self.slide(dimension, 1, f64::from(vertical) * FLIGHT_STEP);
            if vertical < 0 && (contact || self.touches_ground(dimension)) {
                self.flying = false;
                self.grounded = true;
                self.last_jump_press = None;
            }
            return 0.0;
        }

        if jump_requested && self.grounded {
            self.vertical_speed = JUMP_SPEED;
        }
        self.vertical_speed = (self.vertical_speed - GRAVITY).max(-MAX_FALL_SPEED);
        let was_falling = self.vertical_speed < 0.0;
        let vertical_contact = self.slide(dimension, 1, self.vertical_speed);
        self.grounded = was_falling && vertical_contact;
        if vertical_contact {
            self.vertical_speed = 0.0;
        }
        if self.grounded && started_grounded {
            (self.position[0] - x).hypot(self.position[2] - z)
        } else {
            0.0
        }
    }

    fn touches_ground(self, dimension: &Dimension) -> bool {
        let feet = self.position[1];
        let boundary = feet.round();
        if (feet - boundary).abs() > CONTACT_EPSILON {
            return false;
        }
        let [mut x, _, z] = self.bounds().block_ranges();
        let below = boundary as i64 - 1;
        x.any(|x| {
            z.clone().any(|z| {
                !dimension
                    .voxel(BlockPos::new(x, below, z))
                    .is_some_and(|(state, _)| dimension.is_passable(state))
            })
        })
    }

    fn slide(&mut self, dimension: &Dimension, axis: usize, movement: f64) -> bool {
        let allowed = self.bounds().clip_motion(dimension, axis, movement);
        self.position[axis] += allowed;
        allowed != movement
    }

    pub(crate) fn bounds(self) -> Bounds {
        let [x, y, z] = self.position;
        Bounds {
            min: [x - HALF_WIDTH, y, z - HALF_WIDTH],
            max: [x + HALF_WIDTH, y + HEIGHT, z + HALF_WIDTH],
        }
    }

    pub(crate) fn overlaps(self, block: BlockPos) -> bool {
        let [x, y, z] = self.bounds().block_ranges();
        x.contains(&block.x) && y.contains(&block.y) && z.contains(&block.z)
    }

    pub(crate) fn pressure_plate_cells(self) -> impl Iterator<Item = BlockPos> {
        self.bounds().cells()
    }

    pub(crate) fn presses_plate(self, position: BlockPos) -> bool {
        self.bounds().touches_pressure_plate(position)
    }

    pub(crate) fn overlaps_chunk(self, chunk: ChunkPos) -> bool {
        let [mut x, _, z] = self.bounds().block_ranges();
        x.any(|x| z.clone().any(|z| BlockPos::new(x, 0, z).chunk() == chunk))
    }

    pub(crate) fn snapshot(self, entity: EntityId, target: Option<BlockTarget>) -> PlayerSnapshot {
        PlayerSnapshot {
            entity,
            position: self.position,
            yaw: self.input.yaw,
            pitch: self.input.pitch,
            grounded: self.grounded,
            flying: self.flying,
            target,
        }
    }

    pub(crate) fn target_v1(self, dimension: &Dimension) -> Option<BlockTarget> {
        let [x, y, z] = self.position;
        trace_target(
            [x, y + EYE_HEIGHT, z],
            self.input.yaw,
            self.input.pitch,
            |position| {
                dimension
                    .voxel(position)
                    .map(|(state, revision)| (!dimension.is_empty(state), revision))
            },
        )
    }
}

impl Bounds {
    pub(crate) fn clip_motion(self, dimension: &Dimension, axis: usize, movement: f64) -> f64 {
        if movement == 0.0 {
            return 0.0;
        }
        let mut sweep = self;
        if movement > 0.0 {
            sweep.max[axis] += movement;
        } else {
            sweep.min[axis] += movement;
        }

        let mut allowed = movement;
        let [x, y, z] = sweep.block_ranges();
        for bx in x {
            for by in y.clone() {
                for bz in z.clone() {
                    let block = BlockPos::new(bx, by, bz);
                    // Treat unknown chunks as a boundary until authority loads them.
                    if dimension
                        .voxel(block)
                        .is_some_and(|(state, _)| dimension.is_passable(state))
                    {
                        continue;
                    }
                    let cell = [bx, by, bz][axis] as f64;
                    if movement > 0.0 {
                        allowed = allowed.min((cell - self.max[axis]).max(0.0));
                    } else {
                        allowed = allowed.max((cell + 1.0 - self.min[axis]).min(0.0));
                    }
                }
            }
        }
        allowed
    }

    pub(crate) fn cells(self) -> impl Iterator<Item = BlockPos> {
        let [xs, ys, zs] = self.block_ranges();
        xs.flat_map(move |x| {
            let zs = zs.clone();
            ys.clone()
                .flat_map(move |y| zs.clone().map(move |z| BlockPos::new(x, y, z)))
        })
    }

    pub(crate) fn touches_pressure_plate(self, position: BlockPos) -> bool {
        let origin = [position.x as f64, position.y as f64, position.z as f64];
        let low = [0.125, 0.0, 0.125];
        let high = [0.875, 0.25, 0.875];
        (0..3).all(|axis| {
            self.max[axis] > origin[axis] + low[axis] && self.min[axis] < origin[axis] + high[axis]
        })
    }
}

pub(crate) fn trace_target(
    eye: [f64; 3],
    yaw: f64,
    pitch: f64,
    mut voxel: impl FnMut(BlockPos) -> Option<(bool, u64)>,
) -> Option<BlockTarget> {
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let (sin_pitch, cos_pitch) = pitch.sin_cos();
    let direction = [sin_yaw * cos_pitch, sin_pitch, cos_yaw * cos_pitch];

    let mut cell = eye.map(|axis| axis.floor() as i64);
    let mut step = [0; 3];
    let mut next = [f64::INFINITY; 3];
    let mut interval = [f64::INFINITY; 3];
    for axis in 0..3 {
        if direction[axis].abs() <= f64::EPSILON {
            continue;
        }
        step[axis] = if direction[axis] > 0.0 { 1 } else { -1 };
        let boundary = cell[axis] + i64::from(step[axis] > 0);
        next[axis] = (boundary as f64 - eye[axis]) / direction[axis];
        interval[axis] = direction[axis].abs().recip();
    }

    let mut adjacent = None;
    loop {
        let position = to_block(cell);
        let (occupied, revision) = voxel(position)?;
        if occupied {
            let placement = adjacent.and_then(|position| {
                let (occupied, revision) = voxel(position)?;
                (!occupied).then_some(BlockPlacement { position, revision })
            });
            return Some(BlockTarget {
                position,
                revision,
                placement,
            });
        }

        // At an edge or corner, skip cells the ray only touches at one point.
        let distance = next[0].min(next[1]).min(next[2]);
        if distance > BLOCK_REACH_V1 {
            return None;
        }
        let face = next.iter().position(|time| *time == distance)?;
        for axis in 0..3 {
            if (next[axis] - distance).abs() <= CONTACT_EPSILON {
                cell[axis] += step[axis];
                next[axis] += interval[axis];
            }
        }
        let mut neighbor = cell;
        neighbor[face] -= step[face];
        adjacent = Some(to_block(neighbor));
    }
}

fn to_block([x, y, z]: [i64; 3]) -> BlockPos {
    BlockPos::new(x, y, z)
}

fn block_position(position: [f64; 3]) -> BlockPos {
    to_block(position.map(|axis| axis.floor() as i64))
}

#[derive(Clone, Copy)]
pub(crate) struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Bounds {
    fn block_ranges(self) -> [std::ops::Range<i64>; 3] {
        std::array::from_fn(|axis| {
            (self.min[axis] + CONTACT_EPSILON).floor() as i64
                ..(self.max[axis] - CONTACT_EPSILON).ceil() as i64
        })
    }
}
