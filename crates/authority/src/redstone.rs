use std::collections::BTreeSet;
use std::f64::consts::FRAC_PI_2;

use rustcraft_mod_api::{BlockStateSchema, Registrar, Registry};
use rustcraft_world::{
    BlockPos, BlockStateId, HORIZONTAL_NEIGHBORS, TerrainModel, WireCell, WireGeometry,
    stepped_wire,
};

use crate::java_bud::PistonObstruction;
use crate::java_controls::{
    ButtonState, LEGACY_LEVER_NAMES, MountedControl, PlateState, java_control_names,
    java_lever_names,
};
use crate::java_ejector::{Ejector, EjectorKind, java_ejector_names};
use crate::java_redstone::EditNotification;
use crate::java_signal::{DEVICE_FACES, Face, Signal};
use crate::java_wire::{Shape, java_wire_names, parse_wire_name, wire_name};
use crate::{Effect, PistonKind, Rejection, ScheduledOperation, Server};

const DUST: [&str; 16] = [
    "rustcraft_java:redstone",
    "rustcraft_java:redstone_power_1",
    "rustcraft_java:redstone_power_2",
    "rustcraft_java:redstone_power_3",
    "rustcraft_java:redstone_power_4",
    "rustcraft_java:redstone_power_5",
    "rustcraft_java:redstone_power_6",
    "rustcraft_java:redstone_power_7",
    "rustcraft_java:redstone_power_8",
    "rustcraft_java:redstone_power_9",
    "rustcraft_java:redstone_power_10",
    "rustcraft_java:redstone_power_11",
    "rustcraft_java:redstone_power_12",
    "rustcraft_java:redstone_power_13",
    "rustcraft_java:redstone_power_14",
    "rustcraft_java:redstone_power_15",
];
macro_rules! repeater_delay_states {
    ($facing:literal, $delay:literal) => {
        [
            concat!("rustcraft_java:repeater", $facing, $delay),
            concat!("rustcraft_java:repeater", $facing, $delay, "_on"),
            concat!("rustcraft_java:repeater", $facing, $delay, "_locked"),
            concat!("rustcraft_java:repeater", $facing, $delay, "_locked_on"),
        ]
    };
}

macro_rules! repeater_states {
    ($facing:literal) => {
        [
            repeater_delay_states!($facing, ""),
            repeater_delay_states!($facing, "_delay2"),
            repeater_delay_states!($facing, "_delay3"),
            repeater_delay_states!($facing, "_delay4"),
        ]
    };
}

const REPEATERS: [[[&str; 4]; 4]; 4] = [
    repeater_states!("_south"),
    repeater_states!(""),
    repeater_states!("_north"),
    repeater_states!("_west"),
];
const OBSERVERS: [[&str; 2]; 6] = [
    ["rustcraft_java:observer", "rustcraft_java:observer_on"],
    [
        "rustcraft_java:observer_east",
        "rustcraft_java:observer_east_on",
    ],
    [
        "rustcraft_java:observer_north",
        "rustcraft_java:observer_north_on",
    ],
    [
        "rustcraft_java:observer_west",
        "rustcraft_java:observer_west_on",
    ],
    [
        "rustcraft_java:observer_down",
        "rustcraft_java:observer_down_on",
    ],
    [
        "rustcraft_java:observer_up",
        "rustcraft_java:observer_up_on",
    ],
];
macro_rules! comparator_states {
    ($facing:literal) => {
        [
            concat!("rustcraft_java:comparator", $facing),
            concat!("rustcraft_java:comparator", $facing, "_on"),
            concat!("rustcraft_java:comparator", $facing, "_subtract"),
            concat!("rustcraft_java:comparator", $facing, "_subtract_on"),
        ]
    };
}

const COMPARATORS: [[&str; 4]; 4] = [
    comparator_states!("_south"),
    comparator_states!(""),
    comparator_states!("_north"),
    comparator_states!("_west"),
];
const CAKES: [&str; 7] = [
    "rustcraft_java:cake",
    "rustcraft_java:cake_bites_1",
    "rustcraft_java:cake_bites_2",
    "rustcraft_java:cake_bites_3",
    "rustcraft_java:cake_bites_4",
    "rustcraft_java:cake_bites_5",
    "rustcraft_java:cake_bites_6",
];
const PISTONS: [[&str; 2]; 6] = [
    [
        "rustcraft_java:piston_south",
        "rustcraft_java:piston_south_extended",
    ],
    ["rustcraft_java:piston", "rustcraft_java:piston_extended"],
    [
        "rustcraft_java:piston_north",
        "rustcraft_java:piston_north_extended",
    ],
    [
        "rustcraft_java:piston_west",
        "rustcraft_java:piston_west_extended",
    ],
    [
        "rustcraft_java:piston_down",
        "rustcraft_java:piston_down_extended",
    ],
    [
        "rustcraft_java:piston_up",
        "rustcraft_java:piston_up_extended",
    ],
];
const HEADS: [&str; 6] = [
    "rustcraft_java:piston_head_south",
    "rustcraft_java:piston_head_east",
    "rustcraft_java:piston_head_north",
    "rustcraft_java:piston_head_west",
    "rustcraft_java:piston_head_down",
    "rustcraft_java:piston_head_up",
];
const MOVING: [&str; 6] = [
    "rustcraft_java:moving_piston_south",
    "rustcraft_java:moving_piston_east",
    "rustcraft_java:moving_piston_north",
    "rustcraft_java:moving_piston_west",
    "rustcraft_java:moving_piston_down",
    "rustcraft_java:moving_piston_up",
];
const STICKY_PISTONS: [[&str; 2]; 6] = [
    [
        "rustcraft_java:sticky_piston_south",
        "rustcraft_java:sticky_piston_south_extended",
    ],
    [
        "rustcraft_java:sticky_piston",
        "rustcraft_java:sticky_piston_extended",
    ],
    [
        "rustcraft_java:sticky_piston_north",
        "rustcraft_java:sticky_piston_north_extended",
    ],
    [
        "rustcraft_java:sticky_piston_west",
        "rustcraft_java:sticky_piston_west_extended",
    ],
    [
        "rustcraft_java:sticky_piston_down",
        "rustcraft_java:sticky_piston_down_extended",
    ],
    [
        "rustcraft_java:sticky_piston_up",
        "rustcraft_java:sticky_piston_up_extended",
    ],
];
const STICKY_HEADS: [&str; 6] = [
    "rustcraft_java:sticky_piston_head_south",
    "rustcraft_java:sticky_piston_head_east",
    "rustcraft_java:sticky_piston_head_north",
    "rustcraft_java:sticky_piston_head_west",
    "rustcraft_java:sticky_piston_head_down",
    "rustcraft_java:sticky_piston_head_up",
];
const STICKY_MOVING: [&str; 6] = [
    "rustcraft_java:moving_sticky_piston_south",
    "rustcraft_java:moving_sticky_piston_east",
    "rustcraft_java:moving_sticky_piston_north",
    "rustcraft_java:moving_sticky_piston_west",
    "rustcraft_java:moving_sticky_piston_down",
    "rustcraft_java:moving_sticky_piston_up",
];
const POWER_BLOCK: &str = "rustcraft_java:redstone_block";
const REDSTONE_TORCHES: [(Face, [&str; 2]); 5] = [
    (
        Face::Down,
        [
            "rustcraft_java:redstone_torch",
            "rustcraft_java:redstone_torch_off",
        ],
    ),
    (
        Face::South,
        [
            "rustcraft_java:redstone_wall_torch_north",
            "rustcraft_java:redstone_wall_torch_north_off",
        ],
    ),
    (
        Face::East,
        [
            "rustcraft_java:redstone_wall_torch_west",
            "rustcraft_java:redstone_wall_torch_west_off",
        ],
    ),
    (
        Face::North,
        [
            "rustcraft_java:redstone_wall_torch_south",
            "rustcraft_java:redstone_wall_torch_south_off",
        ],
    ),
    (
        Face::West,
        [
            "rustcraft_java:redstone_wall_torch_east",
            "rustcraft_java:redstone_wall_torch_east_off",
        ],
    ),
];
const DIRECTIONS: [[i64; 3]; 6] = [
    HORIZONTAL_NEIGHBORS[0],
    HORIZONTAL_NEIGHBORS[1],
    HORIZONTAL_NEIGHBORS[2],
    HORIZONTAL_NEIGHBORS[3],
    [0, -1, 0],
    [0, 1, 0],
];
const LAMP: [&str; 2] = [
    "rustcraft_java:redstone_lamp",
    "rustcraft_java:redstone_lamp_on",
];
const WORK_LIMIT: usize = 4096;

pub struct WireConnection {
    pub state: BlockStateId,
    pub faces: u8,
    pub wire_power: Option<u8>,
    pub geometry: Option<WireGeometry>,
    pub passable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Repeater {
    /// Direction of the output, opposite the Java BlockState facing.
    pub output: usize,
    pub delay_rt: u8,
    pub powered: bool,
    pub locked: bool,
}

impl Repeater {
    fn new(output: usize) -> Self {
        Self {
            output,
            delay_rt: 1,
            powered: false,
            locked: false,
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        for (output, delays) in REPEATERS.iter().enumerate() {
            for (delay, states) in delays.iter().enumerate() {
                if let Some(state) = states.iter().position(|&entry| entry == name) {
                    return Some(Self {
                        output,
                        delay_rt: delay as u8 + 1,
                        powered: state % 2 != 0,
                        locked: state >= 2,
                    });
                }
            }
        }
        None
    }

    fn name(self) -> &'static str {
        let variant = 2 * usize::from(self.locked) + usize::from(self.powered);
        REPEATERS[self.output][usize::from(self.delay_rt - 1)][variant]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Comparator {
    pub output: usize,
    pub subtract: bool,
    pub powered: bool,
}

impl Comparator {
    fn new(output: usize) -> Self {
        Self {
            output,
            subtract: false,
            powered: false,
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        COMPARATORS.iter().enumerate().find_map(|(output, states)| {
            let state = states.iter().position(|&entry| entry == name)?;
            Some(Self {
                output,
                subtract: state >= 2,
                powered: state % 2 != 0,
            })
        })
    }

    fn name(self) -> &'static str {
        COMPARATORS[self.output][2 * usize::from(self.subtract) + usize::from(self.powered)]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Device {
    Lever(MountedControl),
    Dust(u8, Option<Shape>),
    Lamp(bool),
    Repeater(Repeater),
    Comparator(Comparator),
    Button(ButtonState),
    PressurePlate(PlateState),
    Ejector(Ejector),
    Cake(u8),
    Observer(usize, bool),
    Torch { support: Face, lit: bool },
    Piston(usize, bool, PistonKind),
    Head(usize, PistonKind),
    Moving(usize, PistonKind),
    PowerBlock,
}

type PositionPair = (BlockPos, u8);

impl Device {
    fn from_name(name: &str) -> Option<Self> {
        if !name.starts_with("rustcraft_java:") {
            return None;
        }
        if name == POWER_BLOCK {
            return Some(Self::PowerBlock);
        }
        if name.starts_with("rustcraft_java:lever") {
            return MountedControl::lever_from_name(name).map(Self::Lever);
        }
        if let Some(power) = DUST.iter().position(|&entry| entry == name) {
            return Some(Self::Dust(power as u8, None));
        }
        if let Some((power, shape)) = parse_wire_name(name) {
            return Some(Self::Dust(power, Some(shape)));
        }
        if let Some(index) = LAMP.iter().position(|&entry| entry == name) {
            return Some(Self::Lamp(index != 0));
        }
        if name.starts_with("rustcraft_java:repeater") {
            return Repeater::from_name(name).map(Self::Repeater);
        }
        if name.starts_with("rustcraft_java:comparator") {
            return Comparator::from_name(name).map(Self::Comparator);
        }
        if name.contains("_button") {
            return ButtonState::from_name(name).map(Self::Button);
        }
        if name.contains("_pressure_plate") {
            return PlateState::from_name(name).map(Self::PressurePlate);
        }
        if name.starts_with("rustcraft_java:dispenser")
            || name.starts_with("rustcraft_java:dropper")
        {
            return Ejector::from_name(name).map(Self::Ejector);
        }
        if name.starts_with("rustcraft_java:cake") {
            return CAKES
                .iter()
                .position(|&entry| entry == name)
                .map(|bites| Self::Cake(bites as u8));
        }
        if name.starts_with("rustcraft_java:observer") {
            return OBSERVERS.iter().enumerate().find_map(|(facing, states)| {
                let state = states.iter().position(|&entry| entry == name)?;
                Some(Self::Observer(facing, state != 0))
            });
        }
        if name.starts_with("rustcraft_java:redstone_torch")
            || name.starts_with("rustcraft_java:redstone_wall_torch")
        {
            return REDSTONE_TORCHES.iter().find_map(|&(support, states)| {
                let state = states.iter().position(|&entry| entry == name)?;
                Some(Self::Torch {
                    support,
                    lit: state == 0,
                })
            });
        }
        for (kind, bases, heads, moving) in [
            (PistonKind::Normal, &PISTONS, &HEADS, &MOVING),
            (
                PistonKind::Sticky,
                &STICKY_PISTONS,
                &STICKY_HEADS,
                &STICKY_MOVING,
            ),
        ] {
            for (facing, states) in bases.iter().enumerate() {
                if let Some(extended) = states.iter().position(|&entry| entry == name) {
                    return Some(Self::Piston(facing, extended != 0, kind));
                }
                if heads[facing] == name {
                    return Some(Self::Head(facing, kind));
                }
                if moving[facing] == name {
                    return Some(Self::Moving(facing, kind));
                }
            }
        }
        None
    }

    fn name(self) -> &'static str {
        match self {
            Self::Lever(lever) => lever.lever_name(),
            Self::Dust(power, None) => DUST[usize::from(power)],
            Self::Dust(power, Some(shape)) => wire_name(power, shape),
            Self::Lamp(on) => LAMP[usize::from(on)],
            Self::Repeater(repeater) => repeater.name(),
            Self::Comparator(comparator) => comparator.name(),
            Self::Button(button) => button.name(),
            Self::PressurePlate(plate) => plate.name(),
            Self::Ejector(ejector) => ejector.name(),
            Self::Cake(bites) => CAKES[usize::from(bites)],
            Self::Observer(facing, on) => OBSERVERS[facing][usize::from(on)],
            Self::Torch { support, lit } => {
                REDSTONE_TORCHES
                    .iter()
                    .find(|(mount, _)| *mount == support)
                    .expect("registered torch mount")
                    .1[usize::from(!lit)]
            }
            Self::Piston(facing, extended, PistonKind::Normal) => {
                PISTONS[facing][usize::from(extended)]
            }
            Self::Piston(facing, extended, PistonKind::Sticky) => {
                STICKY_PISTONS[facing][usize::from(extended)]
            }
            Self::Head(facing, PistonKind::Normal) => HEADS[facing],
            Self::Head(facing, PistonKind::Sticky) => STICKY_HEADS[facing],
            Self::Moving(facing, PistonKind::Normal) => MOVING[facing],
            Self::Moving(facing, PistonKind::Sticky) => STICKY_MOVING[facing],
            Self::PowerBlock => POWER_BLOCK,
        }
    }

    pub(crate) fn state(self, registry: &Registry) -> BlockStateId {
        registry.block_id(self.name()).expect("registered device")
    }

    pub(crate) fn thin(self) -> bool {
        self.attachment_face().is_some()
    }

    pub(crate) fn attachment_face(self) -> Option<Face> {
        match self {
            Self::Torch { support, .. } => Some(support),
            Self::Lever(lever) => Some(lever.support()),
            Self::Button(button) => Some(button.control.support()),
            Self::Dust(..)
            | Self::Repeater(_)
            | Self::Comparator(_)
            | Self::PressurePlate(_)
            | Self::Cake(_) => Some(Face::Down),
            _ => None,
        }
    }

    pub(crate) fn wire_ports(self) -> u8 {
        match self {
            Self::Dust(..)
            | Self::Lever(_)
            | Self::PowerBlock
            | Self::Torch { .. }
            | Self::Button(_)
            | Self::PressurePlate(_) => 0b1111,
            Self::Repeater(repeater) => (1 << repeater.output) | (1 << opposite(repeater.output)),
            Self::Comparator(comparator) => {
                (1 << comparator.output) | (1 << opposite(comparator.output))
            }
            Self::Observer(facing, _) if facing < 4 => 1 << opposite(facing),
            _ => 0,
        }
    }

    pub(crate) fn diode_output(self) -> Option<Face> {
        match self {
            Self::Repeater(repeater) => Some(DEVICE_FACES[repeater.output]),
            Self::Comparator(comparator) => Some(DEVICE_FACES[comparator.output]),
            _ => None,
        }
    }

    pub(crate) fn analog_output(self) -> Option<Signal> {
        match self {
            Self::Cake(bites) => Signal::new(14 - 2 * bites),
            _ => None,
        }
    }

    pub(crate) fn control_support_signal(self) -> Option<(Face, Signal)> {
        match self {
            Self::Lever(lever) if lever.powered => Some((lever.support(), Signal::MAX)),
            Self::Button(button) if button.control.powered => {
                Some((button.control.support(), Signal::MAX))
            }
            Self::PressurePlate(plate) if plate.power != Signal::OFF => {
                Some((Face::Down, plate.power))
            }
            _ => None,
        }
    }

    fn output(self, facing: usize) -> u8 {
        match self {
            Self::PowerBlock => 15,
            Self::Lever(lever) if lever.powered => 15,
            Self::Button(button) if button.control.powered => 15,
            Self::PressurePlate(plate) => plate.power.get(),
            Self::Dust(power, _) => power,
            Self::Repeater(repeater) if repeater.powered && repeater.output == facing => 15,
            Self::Observer(observes, true) if opposite(observes) == facing => 15,
            Self::Torch { support, lit: true } if DEVICE_FACES[facing] != support => 15,
            _ => 0,
        }
    }
}

fn block_names() -> impl Iterator<Item = &'static str> {
    DUST.into_iter()
        .chain(LEGACY_LEVER_NAMES)
        .chain(LAMP)
        .chain(
            REPEATERS
                .into_iter()
                .flat_map(|delays| [delays[0][0], delays[0][1]]),
        )
        .chain(PISTONS.into_iter().take(4).flatten())
        .chain(HEADS.into_iter().take(4))
}

fn java_reference_names() -> impl Iterator<Item = &'static str> {
    PISTONS
        .into_iter()
        .skip(4)
        .flatten()
        .chain(HEADS.into_iter().skip(4))
        .chain(MOVING)
        .chain(STICKY_PISTONS.into_iter().flatten())
        .chain(STICKY_HEADS)
        .chain(STICKY_MOVING)
        .chain([POWER_BLOCK, "rustcraft_java:glass"])
        .chain(
            REPEATERS
                .into_iter()
                .flat_map(|delays| delays.into_iter().flatten().skip(2)),
        )
        .chain(OBSERVERS.into_iter().flatten())
        .chain(REDSTONE_TORCHES.into_iter().flat_map(|(_, states)| states))
        .chain(COMPARATORS.into_iter().flatten())
        .chain(CAKES)
        .chain(java_control_names())
        .chain(java_ejector_names())
        .chain(java_lever_names())
        .chain(java_wire_names())
}

pub(crate) fn register(registry: &mut Registrar<'_>) -> Result<(), String> {
    for name in block_names() {
        registry.block(BlockStateSchema { name, version: 1 })?;
    }
    Ok(())
}

pub(crate) fn register_java_reference(registry: &mut Registrar<'_>) -> Result<(), String> {
    for name in java_reference_names() {
        registry.block(BlockStateSchema { name, version: 1 })?;
    }
    Ok(())
}

pub(crate) fn non_solid(registry: &Registry) -> BTreeSet<usize> {
    block_names()
        .chain(java_reference_names())
        .filter(|name| Device::from_name(name).is_some_and(Device::thin))
        .filter_map(|name| registry.block_id(name))
        .map(BlockStateId::registry_index)
        .collect()
}

fn offset(pos: BlockPos, direction: usize) -> Option<BlockPos> {
    let [x, y, z] = DIRECTIONS[direction];
    Some(BlockPos::new(
        pos.x.checked_add(x)?,
        pos.y.checked_add(y)?,
        pos.z.checked_add(z)?,
    ))
}

fn opposite(direction: usize) -> usize {
    match direction {
        0..=3 => (direction + 2) % 4,
        _ => 9 - direction,
    }
}

fn nearest_look_direction(horizontal_direction: usize, yaw: f64, pitch: f64) -> usize {
    let (vertical, horizontal) = pitch.sin_cos();
    let horizontal = horizontal * yaw.sin().abs().max(yaw.cos().abs());
    if vertical.abs() <= horizontal {
        horizontal_direction
    } else if vertical > 0.0 {
        5
    } else {
        4
    }
}

impl Server {
    /// Horizontal visual ports for devices and wire states in the loaded registry.
    pub fn wire_connections(&self) -> impl Iterator<Item = WireConnection> + '_ {
        block_names()
            .chain(java_reference_names())
            .filter_map(|name| {
                let state = self.registry.block_id(name)?;
                let device = Device::from_name(name)?;
                let faces = device.wire_ports();
                (faces != 0).then_some(WireConnection {
                    state,
                    faces,
                    passable: device.thin(),
                    wire_power: match device {
                        Device::Dust(power, _) => Some(power),
                        _ => None,
                    },
                    geometry: match device {
                        Device::Dust(_, shape) => shape.map(Shape::geometry),
                        _ => None,
                    },
                })
            })
    }

    pub fn terrain_models(&self) -> impl Iterator<Item = (BlockStateId, TerrainModel)> + '_ {
        block_names()
            .chain(java_reference_names())
            .filter_map(|name| {
                let model = match Device::from_name(name)? {
                    Device::Lever(lever) => TerrainModel::Lever {
                        mount: lever.mount,
                        facing: lever.facing,
                        powered: lever.powered,
                    },
                    Device::Button(button) => TerrainModel::Button {
                        mount: button.control.mount,
                        facing: button.control.facing,
                        material: button.kind.material(),
                        pressed: button.control.powered,
                    },
                    Device::PressurePlate(plate) => TerrainModel::PressurePlate {
                        material: plate.kind.material(),
                        pressed: plate.power != Signal::OFF,
                    },
                    Device::Ejector(ejector) => TerrainModel::Ejector {
                        facing: ejector.facing,
                        dropper: ejector.kind == EjectorKind::Dropper,
                    },
                    Device::Torch { support, lit } => TerrainModel::RedstoneTorch {
                        wall_facing: DEVICE_FACES[..4]
                            .iter()
                            .position(|&face| face == support.opposite()),
                        lit,
                    },
                    Device::Repeater(repeater) => TerrainModel::Repeater {
                        facing: repeater.output,
                        powered: repeater.powered,
                        delay_rt: repeater.delay_rt,
                        locked: repeater.locked,
                    },
                    Device::Comparator(comparator) => TerrainModel::Comparator {
                        facing: comparator.output,
                        powered: comparator.powered,
                        subtract: comparator.subtract,
                    },
                    Device::Cake(bites) => TerrainModel::Cake { bites },
                    Device::Piston(facing, extended, kind) => TerrainModel::Piston {
                        facing,
                        extended,
                        sticky: kind == PistonKind::Sticky,
                    },
                    Device::Head(facing, kind) | Device::Moving(facing, kind) => {
                        TerrainModel::PistonHead {
                            facing,
                            sticky: kind == PistonKind::Sticky,
                        }
                    }
                    _ => return None,
                };
                Some((self.registry.block_id(name)?, model))
            })
    }

    pub(crate) fn device(&self, id: BlockStateId) -> Option<Device> {
        Device::from_name(self.registry.block(id)?.name)
    }

    pub(crate) fn read_device(&self, position: BlockPos) -> Option<Device> {
        self.device(self.dimension.voxel(position)?.0)
    }

    pub(crate) fn device_item(&self, block: BlockStateId) -> BlockStateId {
        match self.device(block) {
            Some(Device::Lever(_)) => Device::Lever(MountedControl::LEVER_ITEM),
            Some(Device::Dust(..)) => Device::Dust(0, None),
            Some(Device::Lamp(_)) => Device::Lamp(false),
            Some(Device::Repeater(_)) => Device::Repeater(Repeater::new(1)),
            Some(Device::Comparator(_)) => Device::Comparator(Comparator::new(1)),
            Some(Device::Button(button)) => Device::Button(button.item()),
            Some(Device::PressurePlate(plate)) => Device::PressurePlate(PlateState {
                power: Signal::OFF,
                ..plate
            }),
            Some(Device::Ejector(ejector)) => Device::Ejector(ejector.item()),
            Some(Device::Cake(_)) => Device::Cake(0),
            Some(Device::Observer(..)) => Device::Observer(0, false),
            Some(Device::Torch { .. }) => Device::Torch {
                support: Face::Down,
                lit: true,
            },
            Some(Device::Piston(_, _, kind) | Device::Head(_, kind) | Device::Moving(_, kind)) => {
                Device::Piston(1, false, kind)
            }
            Some(Device::PowerBlock) | None => return block,
        }
        .state(&self.registry)
    }

    pub(crate) fn oriented_device(
        &self,
        block: BlockStateId,
        yaw: f64,
        pitch: f64,
    ) -> BlockStateId {
        let direction = (yaw / FRAC_PI_2).round() as usize % 4;
        match self.device(block) {
            Some(Device::Repeater(_)) => {
                Device::Repeater(Repeater::new(direction)).state(&self.registry)
            }
            Some(Device::Comparator(_)) => {
                Device::Comparator(Comparator::new(direction)).state(&self.registry)
            }
            Some(Device::Piston(_, _, kind)) => {
                let look = if self.rules().uses_java_block_events() {
                    nearest_look_direction(direction, yaw, pitch)
                } else {
                    direction
                };
                Device::Piston(opposite(look), false, kind).state(&self.registry)
            }
            Some(Device::Observer(..)) => {
                let facing = nearest_look_direction(direction, yaw, pitch);
                Device::Observer(facing, false).state(&self.registry)
            }
            Some(Device::Ejector(ejector)) => Device::Ejector(Ejector {
                facing: opposite(nearest_look_direction(direction, yaw, pitch)),
                triggered: false,
                ..ejector
            })
            .state(&self.registry),
            _ => block,
        }
    }

    pub(crate) fn has_redstone_support(&self, position: BlockPos) -> bool {
        self.has_redstone_attachment(position, Face::Down)
    }

    pub(crate) fn has_redstone_attachment(&self, position: BlockPos, face: Face) -> bool {
        face.offset(position)
            .and_then(|below| self.dimension.voxel(below))
            .is_some_and(|(block, _)| !self.dimension.is_passable(block))
    }

    pub(crate) fn edit_linked_block(
        &mut self,
        position: BlockPos,
        replacement: BlockStateId,
        revision: u64,
        notifications: crate::java_redstone::EditNotification,
    ) -> Result<bool, Rejection> {
        if matches!(self.device(replacement), Some(Device::Moving(..)))
            || self
                .java_redstone
                .as_ref()
                .is_some_and(|logic| logic.is_moving(position))
        {
            return Err(Rejection::PlacementBlocked);
        }
        let linked = self
            .dimension
            .voxel(position)
            .filter(|&(current, actual)| current != replacement && actual == revision)
            .and_then(|(current, _)| match self.device(current) {
                Some(Device::Piston(facing, true, kind)) => {
                    offset(position, facing).map(|other| (other, Device::Head(facing, kind)))
                }
                Some(Device::Head(facing, kind)) => offset(position, opposite(facing))
                    .map(|other| (other, Device::Piston(facing, true, kind))),
                _ => None,
            });
        let companion = if let Some((other, expected)) = linked {
            let (state, _) = self
                .dimension
                .voxel(other)
                .ok_or(Rejection::InactiveChunk)?;
            (self.device(state) == Some(expected)).then_some(other)
        } else {
            None
        };
        let changed = self.dimension.edit_block_with_notifications(
            position,
            replacement,
            revision,
            notifications,
        )?;
        if let Some(other) = companion {
            let revision = self
                .dimension
                .voxel(other)
                .expect("validated piston companion")
                .1;
            self.dimension
                .edit_block(other, self.dimension.empty_state(), revision)
                .expect("validated piston companion edit");
        }
        Ok(changed)
    }

    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    pub(crate) fn redstone_use_available(&self, state: BlockStateId) -> bool {
        match self.device(state) {
            Some(Device::Lever(_)) => true,
            Some(
                Device::Repeater(_) | Device::Comparator(_) | Device::Button(_) | Device::Dust(..),
            ) => self.rules().uses_java_block_events(),
            _ => false,
        }
    }

    pub(crate) fn use_redstone_device(
        &mut self,
        position: BlockPos,
        revision: u64,
        state: BlockStateId,
    ) -> Result<Option<Effect>, Rejection> {
        if let Some(Device::Comparator(comparator)) = self.device(state) {
            return self.toggle_java_comparator(position, revision, comparator);
        }
        if let Some(Device::Button(button)) = self.device(state) {
            return self.press_java_button(position, revision, button).map(Some);
        }
        if let Some(Device::Dust(power, shape)) = self.device(state)
            && self.rules().uses_java_block_events()
        {
            return self.use_java_wire(position, revision, power, shape);
        }
        let next = match self.device(state) {
            Some(Device::Lever(mut lever)) => {
                lever.powered = !lever.powered;
                Device::Lever(lever)
            }
            Some(Device::Repeater(mut repeater)) if self.rules().uses_java_block_events() => {
                repeater.delay_rt = repeater.delay_rt % 4 + 1;
                Device::Repeater(repeater)
            }
            _ => return Ok(None),
        };
        let changed = self
            .dimension
            .edit_block(position, next.state(&self.registry), revision)?;
        Ok(changed.then_some(Effect::ChunkChanged(position.chunk())))
    }

    fn incoming(
        &self,
        position: BlockPos,
        directions: impl Iterator<Item = usize>,
    ) -> Option<PositionPair> {
        let mut strongest = None;
        for direction in directions {
            if let Some(source) = offset(position, direction)
                && let Some(device) = self.read_device(source)
            {
                let power = device.output(opposite(direction));
                if power > strongest.map_or(0, |(_, strength)| strength) {
                    strongest = Some((source, power));
                }
            }
        }
        strongest
    }

    pub(crate) fn wire_cell(&self, pos: BlockPos) -> Option<WireCell> {
        let (block, _) = self.dimension.voxel(pos)?;
        Some(if matches!(self.device(block), Some(Device::Dust(..))) {
            WireCell::Dust
        } else if self.dimension.is_passable(block) {
            WireCell::Open
        } else {
            WireCell::Solid
        })
    }

    fn incoming_wire(&self, position: BlockPos) -> Option<PositionPair> {
        if self.rules().uses_java_block_events() {
            return self.incoming_java_wire(position);
        }
        let mut strongest = None;
        for direction in 0..4 {
            let Some(mut source) = offset(position, direction) else {
                continue;
            };
            let port = opposite(direction);
            let direct = self
                .read_device(source)
                .filter(|device| device.wire_ports() & (1 << port) != 0);
            let device = if let Some(device) = direct {
                device
            } else if let Some((step, _)) =
                stepped_wire(position, direction, |cell| self.wire_cell(cell))
                && let Some(device) = self.read_device(step)
            {
                source = step;
                device
            } else {
                continue;
            };
            let power = match device {
                Device::Comparator(comparator)
                    if comparator.powered && comparator.output == port =>
                {
                    self.java_comparator_strength(source).get()
                }
                _ => device.output(port),
            }
            .saturating_sub(u8::from(matches!(device, Device::Dust(..))));
            if power > strongest.map_or(0, |(_, strength)| strength) {
                strongest = Some((source, power));
            }
        }
        strongest
    }

    pub(crate) fn settle_redstone(&mut self) {
        if self.java_redstone.is_some() {
            self.dispatch_java_edits();
            return;
        }
        for _ in 0..WORK_LIMIT {
            let Some(changed) = self
                .dimension
                .redstone_edits
                .as_mut()
                .and_then(BTreeSet::pop_first)
            else {
                break;
            };
            self.update_device(changed, None);
            for direction in 0..6 {
                if let Some(neighbor) = offset(changed, direction) {
                    self.update_device(neighbor, Some(changed));
                    if direction < 4 {
                        for next in [4, 5]
                            .into_iter()
                            .filter_map(|vertical| offset(neighbor, vertical))
                        {
                            self.update_device(next, Some(changed));
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn update_device(&mut self, position: BlockPos, cause: Option<BlockPos>) {
        let Some((state, _)) = self.dimension.voxel(position) else {
            return;
        };
        let Some(device) = self.device(state) else {
            return;
        };
        if !self.rules().uses_java_block_events()
            && device.thin()
            && !self.has_redstone_support(position)
        {
            self.write_redstone(position, self.dimension.empty_state());
            return;
        }
        match device {
            Device::Dust(power, shape) => {
                let source = self.incoming_wire(position);
                let target = source.map_or(0, |(_, power)| power);
                if power != target {
                    let trace = source.or_else(|| cause.map(|source| (source, 0)));
                    self.apply_device(position, Device::Dust(target, shape), trace);
                }
            }
            Device::Lamp(lit) if self.rules().uses_java_block_events() => {
                self.update_java_lamp(position, lit);
            }
            Device::Torch { support, lit } => self.update_java_torch(position, support, lit),
            Device::Comparator(comparator) => self.update_java_comparator(position, comparator),
            Device::Ejector(ejector) => self.update_java_ejector(position, ejector, cause),
            Device::Lamp(lit) => {
                let source = self.incoming(position, 0..6);
                if let Some(source) = source {
                    if !lit {
                        self.apply_device(position, Device::Lamp(true), Some(source));
                    }
                } else if lit {
                    self.queue_redstone(position, 2);
                }
            }
            Device::Repeater(repeater) if self.rules().uses_java_block_events() => {
                self.update_java_repeater(position, repeater);
            }
            Device::Repeater(repeater) => {
                if self
                    .incoming(position, std::iter::once(opposite(repeater.output)))
                    .is_some()
                    != repeater.powered
                {
                    self.queue_redstone(position, 1);
                }
            }
            Device::Piston(direction, extended, PistonKind::Normal) => {
                let source = self.incoming(position, (0..6).filter(|side| *side != direction));
                if source.is_some() != extended {
                    self.move_piston(position, direction, source);
                }
            }
            Device::Lever(_)
            | Device::Button(_)
            | Device::PressurePlate(_)
            | Device::Cake(_)
            | Device::Observer(..)
            | Device::Piston(_, _, PistonKind::Sticky)
            | Device::Head(..)
            | Device::Moving(..)
            | Device::PowerBlock => {}
        }
    }

    fn queue_redstone(&mut self, position: BlockPos, redstone_ticks: u8) {
        let operation = ScheduledOperation::RedstoneCheck { position };
        if !self.tasks.pending.values().any(|value| *value == operation) {
            let delay_gt = crate::time::java_redstone_delay(redstone_ticks);
            let _ = self.schedule_operation(delay_gt, operation);
        }
    }

    pub(crate) fn run_redstone_check(&mut self, position: BlockPos) -> Result<Effect, Rejection> {
        let Some(device) = self.read_device(position) else {
            return Ok(Effect::Unchanged);
        };
        let changed = match device {
            Device::Lamp(true) if self.incoming(position, 0..6).is_none() => {
                self.apply_device(position, Device::Lamp(false), None)
            }
            // Old reference checkpoints can still contain a prototype repeater callback.
            Device::Repeater(mut repeater) => {
                let source = self.incoming(position, std::iter::once(opposite(repeater.output)));
                let changed = source.is_some() != repeater.powered;
                repeater.powered = !repeater.powered;
                changed && self.apply_device(position, Device::Repeater(repeater), source)
            }
            _ => false,
        };
        Ok(crate::effect(
            changed,
            Effect::ChunkChanged(position.chunk()),
        ))
    }

    fn apply_device(
        &mut self,
        position: BlockPos,
        device: Device,
        source: Option<PositionPair>,
    ) -> bool {
        let notification =
            if self.rules().uses_java_block_events() && matches!(device, Device::Dust(..)) {
                EditNotification::WirePower
            } else {
                EditNotification::All
            };
        let changed =
            self.write_redstone_with(position, device.state(&self.registry), notification);
        #[cfg(any(debug_assertions, feature = "dev-tools"))]
        if changed && let Some((source, power)) = source {
            self.dimension.redstone_updates.push(crate::RedstoneUpdate {
                source,
                position,
                power,
            });
        }
        #[cfg(not(any(debug_assertions, feature = "dev-tools")))]
        let _ = source;
        changed
    }

    pub(crate) fn write_redstone(&mut self, position: BlockPos, state: BlockStateId) -> bool {
        self.write_redstone_with(position, state, EditNotification::All)
    }

    fn write_redstone_with(
        &mut self,
        position: BlockPos,
        state: BlockStateId,
        notification: EditNotification,
    ) -> bool {
        self.dimension.voxel(position).is_some_and(|(_, revision)| {
            self.dimension
                .edit_block_with_notifications(position, state, revision, notification)
                .unwrap_or(false)
        })
    }

    fn move_piston(&mut self, base: BlockPos, direction: usize, source: Option<PositionPair>) {
        let Some(head) = offset(base, direction) else {
            return;
        };
        if source.is_none() {
            if self.read_device(head) == Some(Device::Head(direction, PistonKind::Normal)) {
                self.write_redstone(head, self.dimension.empty_state());
            }
            self.apply_device(
                base,
                Device::Piston(direction, false, PistonKind::Normal),
                None,
            );
            return;
        }
        let Ok(moves) = self.piston_push_plan(head, direction) else {
            return;
        };
        for (destination, state) in moves.into_iter().rev() {
            self.write_redstone(destination, state);
        }
        self.apply_device(head, Device::Head(direction, PistonKind::Normal), None);
        self.apply_device(
            base,
            Device::Piston(direction, true, PistonKind::Normal),
            source,
        );
    }

    pub(crate) fn piston_push_plan(
        &self,
        head: BlockPos,
        direction: usize,
    ) -> Result<Vec<(BlockPos, BlockStateId)>, PistonObstruction> {
        let mut moves = Vec::new();
        let mut cursor = head;
        loop {
            let state = self.piston_block(cursor)?;
            if self
                .player_body()
                .is_some_and(|(_, body)| body.overlaps(cursor))
            {
                return Err(PistonObstruction::Player(cursor));
            }
            if self.dimension.is_empty(state)
                || matches!(self.device(state), Some(Device::Dust(..)))
            {
                break;
            }
            if moves.len() == 12 {
                return Err(PistonObstruction::PushLimit(cursor));
            }
            if !self.piston_pushable(state) {
                return Err(PistonObstruction::Immovable {
                    position: cursor,
                    state: self.registry.block(state).map(|block| block.name),
                });
            }
            let destination =
                offset(cursor, direction).ok_or(PistonObstruction::WorldBoundary(cursor))?;
            moves.push((destination, state));
            cursor = destination;
        }
        Ok(moves)
    }

    pub(crate) fn piston_block(
        &self,
        position: BlockPos,
    ) -> Result<BlockStateId, PistonObstruction> {
        self.dimension
            .voxel(position)
            .map(|(state, _)| state)
            .ok_or_else(|| {
                if self
                    .dimension
                    .height
                    .is_some_and(|range| !range.contains(position.y))
                {
                    PistonObstruction::WorldBoundary(position)
                } else {
                    PistonObstruction::InactiveChunk(position)
                }
            })
    }

    pub(crate) fn piston_pushable(&self, state: BlockStateId) -> bool {
        self.registry.block(state).is_some_and(|block| {
            matches!(
                block.name,
                "rustcraft_java:smooth_stone"
                    | "rustcraft_java:dirt"
                    | "rustcraft_java:grass_block"
                    | "rustcraft_java:redstone_block"
                    | "rustcraft_java:glass"
            )
        })
    }
}
