//! Read-only signal queries for the Java 1.20.1 reference rules.
//! The caller supplies cells from a single consistent world view.

use rustcraft_world::BlockPos;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    West,
    East,
    Down,
    Up,
    North,
    South,
}

/// Direction indices shared by registered device states and terrain models.
pub(crate) const DEVICE_FACES: [Face; 6] = [
    Face::South,
    Face::East,
    Face::North,
    Face::West,
    Face::Down,
    Face::Up,
];

impl Face {
    pub(crate) const JAVA_ORDER: [Self; 6] = [
        Self::Down,
        Self::Up,
        Self::North,
        Self::South,
        Self::West,
        Self::East,
    ];

    pub(crate) const ALL: [Self; 6] = [
        Self::West,
        Self::East,
        Self::Down,
        Self::Up,
        Self::North,
        Self::South,
    ];

    pub const fn opposite(self) -> Self {
        match self {
            Self::West => Self::East,
            Self::East => Self::West,
            Self::Down => Self::Up,
            Self::Up => Self::Down,
            Self::North => Self::South,
            Self::South => Self::North,
        }
    }

    pub fn offset(self, pos: BlockPos) -> Option<BlockPos> {
        let [x, y, z] = match self {
            Self::West => [-1, 0, 0],
            Self::East => [1, 0, 0],
            Self::Down => [0, -1, 0],
            Self::Up => [0, 1, 0],
            Self::North => [0, 0, -1],
            Self::South => [0, 0, 1],
        };
        Some(BlockPos::new(
            pos.x.checked_add(x)?,
            pos.y.checked_add(y)?,
            pos.z.checked_add(z)?,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Signal(u8);

impl Signal {
    pub const OFF: Self = Self(0);
    pub const MAX: Self = Self(15);

    pub const fn new(value: u8) -> Option<Self> {
        if value <= Self::MAX.0 {
            Some(Self(value))
        } else {
            None
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

/// The shape subsystem supplies the horizontal connections of the current wire state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WirePorts {
    pub north: bool,
    pub south: bool,
    pub west: bool,
    pub east: bool,
}

impl WirePorts {
    fn emits(self, toward: Face) -> bool {
        match toward {
            Face::West => self.west,
            Face::East => self.east,
            Face::North => self.north,
            Face::South => self.south,
            Face::Down => true,
            Face::Up => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Emitter {
    #[default]
    None,
    RedstoneBlock,
    Lever {
        powered: bool,
        support: Face,
    },
    Button {
        support: Face,
        powered: bool,
    },
    PressurePlate {
        signal: Signal,
    },
    Wire {
        power: Signal,
        ports: WirePorts,
    },
    Repeater {
        output: Face,
    },
    Comparator {
        output: Face,
        signal: Signal,
    },
    Observer {
        output: Face,
    },
    Torch {
        support: Face,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SignalCell {
    pub conductor: bool,
    pub emitter: Emitter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerContext {
    Normal,
    WireExternalInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerOutput {
    pub normal: Signal,
    pub direct: Signal,
    /// Direct signal received from adjacent emitters and passed through this cell.
    /// `None` indicates an incomplete neighbor view.
    pub conducted: Option<Signal>,
}

impl PowerOutput {
    pub fn emitted(self) -> Option<Signal> {
        if self.normal == Signal::MAX {
            Some(self.normal)
        } else {
            self.conducted.map(|conducted| conducted.max(self.normal))
        }
    }
}

impl Emitter {
    fn outputs(self, toward: Face, context: PowerContext) -> (Signal, Signal) {
        use Signal as S;
        match self {
            Self::RedstoneBlock => (S::MAX, S::OFF),
            Self::Lever {
                powered: true,
                support,
            }
            | Self::Button {
                powered: true,
                support,
            } => (S::MAX, if toward == support { S::MAX } else { S::OFF }),
            Self::PressurePlate { signal } => {
                (signal, if toward == Face::Down { signal } else { S::OFF })
            }
            Self::Wire { power, ports }
                if context == PowerContext::Normal && ports.emits(toward) =>
            {
                (power, power)
            }
            Self::Repeater { output } | Self::Observer { output } if output == toward => {
                (S::MAX, S::MAX)
            }
            Self::Comparator { output, signal } if output == toward => (signal, signal),
            Self::Torch { support } if support != toward => {
                (S::MAX, if toward == Face::Up { S::MAX } else { S::OFF })
            }
            _ => (S::OFF, S::OFF),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivationPower {
    Direct(BlockPos),
    Quasi(BlockPos),
    Unpowered,
    Unavailable,
}

pub type PistonPower = ActivationPower;

impl ActivationPower {
    pub(crate) const fn powered(self) -> Option<bool> {
        match self {
            Self::Direct(_) | Self::Quasi(_) => Some(true),
            Self::Unpowered => Some(false),
            Self::Unavailable => None,
        }
    }
}

/// Query the activation condition without delivering an update to the piston.
pub fn piston_power(
    base: BlockPos,
    facing: Face,
    cell: impl Fn(BlockPos) -> Option<SignalCell>,
) -> PistonPower {
    let mut unavailable = false;
    let mut powered = |position, toward| {
        let output = power_output(position, toward, PowerContext::Normal, &cell);
        match output {
            Some(output) if output.normal != Signal::OFF => true,
            Some(output) if output.conducted.is_some() => output.conducted != Some(Signal::OFF),
            _ => {
                unavailable = true;
                false
            }
        }
    };
    for side in Face::JAVA_ORDER.into_iter().filter(|side| *side != facing) {
        if let Some(source) = side.offset(base)
            && powered(source, side.opposite())
        {
            return PistonPower::Direct(source);
        }
    }
    if powered(base, Face::Up) {
        return PistonPower::Direct(base);
    }
    if let Some(above) = Face::Up.offset(base) {
        for side in Face::JAVA_ORDER
            .into_iter()
            .filter(|side| *side != Face::Down)
        {
            if let Some(source) = side.offset(above)
                && powered(source, side.opposite())
            {
                return PistonPower::Quasi(source);
            }
        }
    }
    if unavailable {
        PistonPower::Unavailable
    } else {
        PistonPower::Unpowered
    }
}

/// Query a source towards a receiver. Missing cells stay distinguishable from air.
/// Conduction reads nearby direct outputs and cannot recursively conduct power.
pub fn power_output(
    source: BlockPos,
    toward: Face,
    context: PowerContext,
    cell: impl Fn(BlockPos) -> Option<SignalCell>,
) -> Option<PowerOutput> {
    let source_state = cell(source)?;
    let (normal, direct) = source_state.emitter.outputs(toward, context);
    let conducted = if source_state.conductor {
        let mut maximum = Signal::OFF;
        let mut missing = false;
        for direction in Face::ALL {
            if let Some(state) = direction.offset(source).and_then(&cell) {
                let (_, direct) = state.emitter.outputs(direction.opposite(), context);
                maximum = maximum.max(direct);
            } else {
                missing = true;
            }
        }
        (!missing || maximum == Signal::MAX).then_some(maximum)
    } else {
        Some(Signal::OFF)
    };
    Some(PowerOutput {
        normal,
        direct,
        conducted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_missing_conductor_neighbors_without_manufacturing_power() {
        let source = BlockPos::new(0, 0, 0);
        let conductor = SignalCell {
            conductor: true,
            ..Default::default()
        };
        let cell = |pos| (pos == source).then_some(conductor);
        let output = power_output(source, Face::East, PowerContext::Normal, cell).unwrap();
        assert_eq!(output.normal, Signal::OFF);
        assert_eq!(output.direct, Signal::OFF);
        assert_eq!(output.emitted(), None);
        assert!(
            power_output(
                BlockPos::new(1, 0, 0),
                Face::East,
                PowerContext::Normal,
                cell
            )
            .is_none()
        );
    }

    #[test]
    fn excludes_wire_output_from_wire_external_input_through_a_conductor() {
        let source = BlockPos::new(0, 0, 0);
        let conductor = BlockPos::new(1, 0, 0);
        let signal = Signal::new(14).unwrap();
        assert!(Signal::new(16).is_none());
        let cell = |pos| {
            Some(if pos == source {
                SignalCell {
                    conductor: false,
                    emitter: Emitter::Wire {
                        power: signal,
                        ports: WirePorts {
                            east: true,
                            ..Default::default()
                        },
                    },
                }
            } else {
                SignalCell {
                    conductor: pos == conductor,
                    ..Default::default()
                }
            })
        };
        let normal = power_output(conductor, Face::East, PowerContext::Normal, cell).unwrap();
        assert_eq!(normal.emitted(), Some(signal));
        let external =
            power_output(conductor, Face::East, PowerContext::WireExternalInput, cell).unwrap();
        assert_eq!(external.emitted(), Some(Signal::OFF));
    }
}
