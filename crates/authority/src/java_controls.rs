use rustcraft_world::{BlockPos, BlockStateId, ControlMaterial, ControlMount};

use crate::java_block_ticks::{BlockTickPriority, ScheduledBlockTick};
use crate::java_redstone::JavaBlockTickKind;
use crate::java_signal::{DEVICE_FACES, Face, Signal};
use crate::player::Bounds;
use crate::player::Player;
use crate::redstone::Device;
use crate::{Effect, Rejection, Server};

macro_rules! control_state {
    ($control:literal, $variant:literal) => {
        [
            concat!("rustcraft_java:", $control, $variant),
            concat!("rustcraft_java:", $control, $variant, "_on"),
        ]
    };
}

macro_rules! control_mounts {
    ($control:literal, $wall_north:literal, $floor_south:literal) => {
        [
            [
                control_state!($control, "_wall_south"),
                control_state!($control, "_wall_east"),
                control_state!($control, $wall_north),
                control_state!($control, "_wall_west"),
            ],
            [
                control_state!($control, $floor_south),
                control_state!($control, "_floor_east"),
                control_state!($control, "_floor_north"),
                control_state!($control, "_floor_west"),
            ],
            [
                control_state!($control, "_ceiling_south"),
                control_state!($control, "_ceiling_east"),
                control_state!($control, "_ceiling_north"),
                control_state!($control, "_ceiling_west"),
            ],
        ]
    };
}

macro_rules! weighted_states {
    ($plate:literal; $($power:literal),+) => {
        [concat!("rustcraft_java:", $plate), $(concat!("rustcraft_java:", $plate, "_power_", $power)),+]
    };
}

macro_rules! weighted_plate {
    ($plate:literal) => {
        weighted_states!($plate; "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15")
    };
}

const STONE_BUTTONS: [[[&str; 2]; 4]; 3] = control_mounts!("stone_button", "", "_floor_south");
const OAK_BUTTONS: [[[&str; 2]; 4]; 3] = control_mounts!("oak_button", "", "_floor_south");
const LEVERS: [[[&str; 2]; 4]; 3] = control_mounts!("lever", "_wall_north", "");
pub(crate) const LEGACY_LEVER_NAMES: [&str; 2] = LEVERS[ControlMount::Floor as usize][0];
const STONE_PLATES: [&str; 2] = [
    "rustcraft_java:stone_pressure_plate",
    "rustcraft_java:stone_pressure_plate_on",
];
const OAK_PLATES: [&str; 2] = [
    "rustcraft_java:oak_pressure_plate",
    "rustcraft_java:oak_pressure_plate_on",
];
const LIGHT_PLATES: [&str; 16] = weighted_plate!("light_weighted_pressure_plate");
const HEAVY_PLATES: [&str; 16] = weighted_plate!("heavy_weighted_pressure_plate");
const MOUNTS: [ControlMount; 3] = [
    ControlMount::Wall,
    ControlMount::Floor,
    ControlMount::Ceiling,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MountedControl {
    pub mount: ControlMount,
    pub facing: usize,
    pub powered: bool,
}

impl MountedControl {
    pub(crate) const LEVER_ITEM: Self = Self {
        mount: ControlMount::Floor,
        facing: 0,
        powered: false,
    };

    fn from_name(name: &str, names: &[[[&str; 2]; 4]; 3]) -> Option<Self> {
        for (mount, facings) in MOUNTS.into_iter().zip(names) {
            for (facing, states) in facings.iter().enumerate() {
                if let Some(powered) = states.iter().position(|&entry| entry == name) {
                    return Some(Self {
                        mount,
                        facing,
                        powered: powered != 0,
                    });
                }
            }
        }
        None
    }

    fn placed_on(support: Face, yaw: f64) -> Self {
        let (mount, facing) = match support {
            Face::Down => (ControlMount::Floor, horizontal_facing(yaw)),
            Face::Up => (ControlMount::Ceiling, horizontal_facing(yaw)),
            _ => (
                ControlMount::Wall,
                DEVICE_FACES
                    .iter()
                    .position(|&face| face == support.opposite())
                    .unwrap(),
            ),
        };
        Self {
            mount,
            facing,
            powered: false,
        }
    }

    pub(crate) fn lever_from_name(name: &str) -> Option<Self> {
        Self::from_name(name, &LEVERS)
    }

    pub(crate) fn lever_name(self) -> &'static str {
        LEVERS[self.mount as usize][self.facing][usize::from(self.powered)]
    }

    pub(crate) fn support(self) -> Face {
        match self.mount {
            ControlMount::Floor => Face::Down,
            ControlMount::Wall => DEVICE_FACES[self.facing].opposite(),
            ControlMount::Ceiling => Face::Up,
        }
    }
}

fn horizontal_facing(yaw: f64) -> usize {
    ((yaw / std::f64::consts::FRAC_PI_2 + 0.5).floor() as i32).rem_euclid(4) as usize
}

pub(crate) fn java_lever_names() -> impl Iterator<Item = &'static str> {
    LEVERS
        .into_iter()
        .flatten()
        .filter(|states| *states != LEGACY_LEVER_NAMES)
        .flatten()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ButtonKind {
    Stone,
    Oak,
}

impl ButtonKind {
    const ALL: [Self; 2] = [Self::Stone, Self::Oak];

    fn names(self) -> &'static [[[&'static str; 2]; 4]; 3] {
        match self {
            Self::Stone => &STONE_BUTTONS,
            Self::Oak => &OAK_BUTTONS,
        }
    }

    fn callback(self) -> JavaBlockTickKind {
        match self {
            Self::Stone => JavaBlockTickKind::StoneButton,
            Self::Oak => JavaBlockTickKind::OakButton,
        }
    }

    fn press_time(self) -> u64 {
        match self {
            Self::Stone => 20,
            Self::Oak => 30,
        }
    }

    pub(crate) fn material(self) -> ControlMaterial {
        match self {
            Self::Stone => ControlMaterial::Stone,
            Self::Oak => ControlMaterial::Oak,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ButtonState {
    pub kind: ButtonKind,
    pub control: MountedControl,
}

impl ButtonState {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        ButtonKind::ALL.into_iter().find_map(|kind| {
            let control = MountedControl::from_name(name, kind.names())?;
            Some(Self { kind, control })
        })
    }

    pub(crate) fn name(self) -> &'static str {
        let MountedControl {
            mount,
            facing,
            powered,
        } = self.control;
        self.kind.names()[mount as usize][facing][usize::from(powered)]
    }

    pub(crate) fn item(self) -> Self {
        Self {
            control: MountedControl {
                mount: ControlMount::Wall,
                facing: 2,
                powered: false,
            },
            ..self
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlateKind {
    Stone,
    Oak,
    Light,
    Heavy,
}

impl PlateKind {
    const ALL: [Self; 4] = [Self::Stone, Self::Oak, Self::Light, Self::Heavy];

    fn names(self) -> &'static [&'static str] {
        match self {
            Self::Stone => &STONE_PLATES,
            Self::Oak => &OAK_PLATES,
            Self::Light => &LIGHT_PLATES,
            Self::Heavy => &HEAVY_PLATES,
        }
    }

    fn capacity(self) -> Option<usize> {
        match self {
            Self::Stone | Self::Oak => None,
            Self::Light => Some(15),
            Self::Heavy => Some(150),
        }
    }

    fn signal(self, entities: usize) -> Signal {
        let power = if let Some(capacity) = self.capacity() {
            (entities.min(capacity) * 15).div_ceil(capacity)
        } else {
            15 * usize::from(entities != 0)
        };
        Signal::new(power as u8).unwrap()
    }

    fn callback(self) -> JavaBlockTickKind {
        match self {
            Self::Stone => JavaBlockTickKind::StonePressurePlate,
            Self::Oak => JavaBlockTickKind::OakPressurePlate,
            Self::Light => JavaBlockTickKind::LightPressurePlate,
            Self::Heavy => JavaBlockTickKind::HeavyPressurePlate,
        }
    }

    fn recheck_period(self) -> u64 {
        if self.capacity().is_some() { 10 } else { 20 }
    }

    pub(crate) fn material(self) -> ControlMaterial {
        match self {
            Self::Stone => ControlMaterial::Stone,
            Self::Oak => ControlMaterial::Oak,
            Self::Light => ControlMaterial::Gold,
            Self::Heavy => ControlMaterial::Iron,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlateState {
    pub kind: PlateKind,
    pub power: Signal,
}

impl PlateState {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        PlateKind::ALL.into_iter().find_map(|kind| {
            let index = kind.names().iter().position(|&entry| entry == name)?;
            let power = if kind.capacity().is_some() {
                Signal::new(index as u8).unwrap()
            } else {
                kind.signal(index)
            };
            Some(Self { kind, power })
        })
    }

    pub(crate) fn name(self) -> &'static str {
        let index = if self.kind.capacity().is_some() {
            usize::from(self.power.get())
        } else {
            usize::from(self.power != Signal::OFF)
        };
        self.kind.names()[index]
    }
}

pub(crate) fn java_control_names() -> impl Iterator<Item = &'static str> {
    ButtonKind::ALL
        .into_iter()
        .flat_map(|kind| kind.names().iter().flatten().flatten().copied())
        .chain(
            PlateKind::ALL
                .into_iter()
                .flat_map(|kind| kind.names().iter().copied()),
        )
}

impl Server {
    pub(crate) fn orient_java_button(
        &self,
        mut button: ButtonState,
        position: BlockPos,
        clicked: BlockPos,
        yaw: f64,
    ) -> Option<BlockStateId> {
        let support = Face::ALL
            .into_iter()
            .find(|face| face.offset(position) == Some(clicked))?;
        button.control = MountedControl::placed_on(support, yaw);
        Some(Device::Button(button).state(&self.registry))
    }

    pub(crate) fn orient_java_lever(
        &self,
        position: BlockPos,
        clicked: BlockPos,
        yaw: f64,
        pitch: f64,
    ) -> Option<BlockStateId> {
        let support = self.preferred_java_attachment(position, clicked, yaw, pitch, true)?;
        Some(Device::Lever(MountedControl::placed_on(support, yaw)).state(&self.registry))
    }

    pub(crate) fn preferred_java_attachment(
        &self,
        position: BlockPos,
        clicked: BlockPos,
        yaw: f64,
        pitch: f64,
        allow_ceiling: bool,
    ) -> Option<Face> {
        let (sy, cy) = yaw.sin_cos();
        let (sp, cp) = pitch.sin_cos();
        let mut supports = [
            (Face::Down, -sp),
            (Face::East, sy * cp),
            (Face::West, -sy * cp),
            (Face::South, cy * cp),
            (Face::North, -cy * cp),
            (Face::Up, sp),
        ];
        let is_clicked = |face: Face| face.offset(position) == Some(clicked);
        supports.sort_by(|&(a, a_alignment), &(b, b_alignment)| {
            is_clicked(b)
                .cmp(&is_clicked(a))
                .then_with(|| b_alignment.total_cmp(&a_alignment))
        });
        supports.into_iter().map(|(face, _)| face).find(|&face| {
            (allow_ceiling || face != Face::Up) && self.has_redstone_attachment(position, face)
        })
    }

    pub(crate) fn press_java_button(
        &mut self,
        position: BlockPos,
        revision: u64,
        mut button: ButtonState,
    ) -> Result<Effect, Rejection> {
        if button.control.powered {
            return Ok(Effect::Unchanged);
        }
        button.control.powered = true;
        let state = Device::Button(button).state(&self.registry);
        self.dimension.edit_block(position, state, revision)?;
        self.dispatch_java_edits();
        self.schedule_java_block_tick(
            position,
            button.kind.callback(),
            BlockTickPriority::Normal,
            button.kind.press_time(),
        );
        Ok(Effect::ChunkChanged(position.chunk()))
    }

    pub(crate) fn run_java_control(&mut self, tick: ScheduledBlockTick<JavaBlockTickKind>) {
        match self.read_device(tick.position) {
            Some(Device::Button(mut button))
                if button.control.powered && button.kind.callback() == tick.kind =>
            {
                button.control.powered = false;
                self.write_redstone(tick.position, Device::Button(button).state(&self.registry));
            }
            Some(Device::PressurePlate(plate))
                if plate.power != Signal::OFF && plate.kind.callback() == tick.kind =>
            {
                self.update_java_pressure_plate(tick.position, plate);
            }
            _ => {}
        }
    }

    pub(crate) fn touch_java_pressure_plates(&mut self, player: Player) {
        for position in player.pressure_plate_cells() {
            if let Some(Device::PressurePlate(plate)) = self.read_device(position)
                && plate.power == Signal::OFF
                && player.presses_plate(position)
            {
                self.update_java_pressure_plate(position, plate);
            }
        }
    }

    pub(crate) fn touch_java_item_pressure_plates(&mut self, bounds: Bounds) {
        for position in bounds.cells() {
            if let Some(Device::PressurePlate(plate)) = self.read_device(position)
                && plate.kind != PlateKind::Stone
                && plate.power == Signal::OFF
                && bounds.touches_pressure_plate(position)
            {
                self.update_java_pressure_plate(position, plate);
            }
        }
    }

    fn update_java_pressure_plate(&mut self, position: BlockPos, plate: PlateState) {
        let players = usize::from(
            self.player_body()
                .is_some_and(|(_, player)| player.presses_plate(position)),
        );
        let items = if plate.kind == PlateKind::Stone {
            0
        } else {
            self.dimension
                .entities
                .items()
                .filter(|(_, item)| item.bounds().touches_pressure_plate(position))
                .count()
        };
        let occupants = players + items;
        let strength = plate.kind.signal(occupants);
        if strength != plate.power {
            let device = Device::PressurePlate(PlateState {
                power: strength,
                ..plate
            });
            self.write_redstone(position, device.state(&self.registry));
            self.dispatch_java_edits();
        }
        if strength != Signal::OFF {
            self.schedule_java_block_tick(
                position,
                plate.kind.callback(),
                BlockTickPriority::Normal,
                plate.kind.recheck_period(),
            );
        }
    }
}
