use std::sync::OnceLock;

use rustcraft_world::{BlockPos, BlockStateId, WireGeometry};

use crate::java_redstone::EditNotification;
use crate::java_signal::{DEVICE_FACES, Face, WirePorts};
use crate::redstone::Device;
use crate::{Effect, Rejection, Server};

const SHAPE_COUNT: usize = 81;
const WIRE_PREFIX: &str = "rustcraft_java:wire_";
const SIDES: [Face; 4] = [
    DEVICE_FACES[0],
    DEVICE_FACES[1],
    DEVICE_FACES[2],
    DEVICE_FACES[3],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Connection {
    None,
    Side,
    Up,
}

impl Connection {
    fn connected(self) -> bool {
        self != Self::None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shape([Connection; 4]);

impl Shape {
    pub(crate) const DOT: Self = Self([Connection::None; 4]);
    const CROSS: Self = Self([Connection::Side; 4]);

    fn from_index(mut index: usize) -> Self {
        Self(std::array::from_fn(|_| {
            let connection = [Connection::None, Connection::Side, Connection::Up][index % 3];
            index /= 3;
            connection
        }))
    }

    fn index(self) -> usize {
        self.0
            .into_iter()
            .rev()
            .fold(0, |index, side| 3 * index + side as usize)
    }

    fn is_cross(self) -> bool {
        self.0.into_iter().all(Connection::connected)
    }

    fn complete_line(mut self, keep_dot: bool) -> Self {
        if self == Self::DOT && keep_dot {
            return self;
        }
        let [south, east, north, west] = self.0.map(Connection::connected);
        for (index, connected) in [!(east || west), !(south || north)].into_iter().enumerate() {
            if connected {
                for direction in [index, index + 2] {
                    if self.0[direction] == Connection::None {
                        self.0[direction] = Connection::Side;
                    }
                }
            }
        }
        self
    }

    pub(crate) fn geometry(self) -> WireGeometry {
        let mut geometry = WireGeometry::default();
        for (direction, connection) in self.0.into_iter().enumerate() {
            if connection.connected() {
                geometry.ports |= 1 << direction;
            }
            if connection == Connection::Up {
                geometry.ascending |= 1 << direction;
            }
        }
        geometry
    }

    fn output_ports(self) -> WirePorts {
        let [south, east, north, west] = self.0.map(Connection::connected);
        WirePorts {
            north,
            south,
            west,
            east,
        }
    }
}

fn names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        (0..=15)
            .flat_map(|power| {
                (0..SHAPE_COUNT).map(move |shape| format!("{WIRE_PREFIX}{power}_{shape}"))
            })
            .collect()
    })
}

pub(crate) fn java_wire_names() -> impl Iterator<Item = &'static str> {
    names().iter().map(String::as_str)
}

pub(crate) fn parse_wire_name(name: &str) -> Option<(u8, Shape)> {
    let (power, shape) = name.strip_prefix(WIRE_PREFIX)?.split_once('_')?;
    let power = power.parse::<u8>().ok()?;
    let shape = shape.parse::<usize>().ok()?;
    (power <= 15 && shape < SHAPE_COUNT).then(|| (power, Shape::from_index(shape)))
}

pub(crate) fn wire_name(power: u8, shape: Shape) -> &'static str {
    &names()[usize::from(power) * SHAPE_COUNT + shape.index()]
}

pub(crate) fn wire_power_notice_centers(position: BlockPos) -> [Option<BlockPos>; 7] {
    let mut centers = [Some(position); 7];
    for (center, face) in centers[1..].iter_mut().zip(Face::JAVA_ORDER) {
        *center = face.offset(position);
    }
    // The seven centers fit in the initial 16 buckets of the Java 17 set.
    // Stable ordering retains insertion order for centers in the same bucket.
    centers.sort_by_key(|center| {
        center.map(|point| {
            let hash = (point.z as u32)
                .wrapping_mul(31)
                .wrapping_add(point.y as u32)
                .wrapping_mul(31)
                .wrapping_add(point.x as u32);
            (hash ^ (hash >> 16)) & 15
        })
    });
    centers
}

impl Server {
    fn wire_connection(
        &self,
        position: BlockPos,
        direction: usize,
        climb: bool,
    ) -> Option<Connection> {
        let face = SIDES[direction];
        let side = face.offset(position)?;
        let block = self.wire_block_at(side)?;
        if climb
            && !self.dimension.is_passable(block)
            && Face::Up
                .offset(side)
                .is_some_and(|above| self.wire_at(above).is_some())
        {
            return Some(Connection::Up);
        }
        if self
            .device(block)
            .is_some_and(|device| device.wire_ports() & (1 << ((direction + 2) % 4)) != 0)
        {
            return Some(Connection::Side);
        }
        if !self.java_conductor(block)
            && Face::Down
                .offset(side)
                .is_some_and(|below| self.wire_at(below).is_some())
        {
            return Some(Connection::Side);
        }
        Some(Connection::None)
    }

    fn wire_block_at(&self, position: BlockPos) -> Option<BlockStateId> {
        if self
            .dimension
            .height
            .is_some_and(|range| !range.contains(position.y))
        {
            Some(self.dimension.empty_state())
        } else {
            self.dimension.voxel(position).map(|(block, _)| block)
        }
    }

    fn wire_climb_open(&self, position: BlockPos) -> bool {
        Face::Up
            .offset(position)
            .and_then(|above| self.wire_block_at(above))
            .is_some_and(|block| !self.java_conductor(block))
    }

    fn wire_at(&self, position: BlockPos) -> Option<u8> {
        match self.read_device(position)? {
            Device::Dust(power, _) => Some(power),
            _ => None,
        }
    }

    fn java_wire_shape(&self, position: BlockPos, previous: Shape) -> Shape {
        let climb = self.wire_climb_open(position);
        Shape(std::array::from_fn(|direction| {
            self.wire_connection(position, direction, climb)
                .unwrap_or(previous.0[direction])
        }))
        .complete_line(previous == Shape::DOT)
    }

    pub(crate) fn java_wire_output_ports(
        &self,
        position: BlockPos,
        shape: Option<Shape>,
    ) -> WirePorts {
        self.java_wire_shape(position, shape.unwrap_or(Shape::CROSS))
            .output_ports()
    }

    pub(crate) fn placed_java_wire(&self, position: BlockPos) -> BlockStateId {
        Device::Dust(0, Some(self.java_wire_shape(position, Shape::CROSS))).state(&self.registry)
    }

    pub(crate) fn use_java_wire(
        &mut self,
        position: BlockPos,
        revision: u64,
        power: u8,
        shape: Option<Shape>,
    ) -> Result<Option<Effect>, Rejection> {
        let current = shape.unwrap_or_else(|| self.java_wire_shape(position, Shape::CROSS));
        let requested = if current == Shape::DOT {
            Shape::CROSS
        } else if current.is_cross() {
            Shape::DOT
        } else {
            return Ok(None);
        };
        let shape = self.java_wire_shape(position, requested);
        if shape == current {
            return Ok(None);
        }
        let block = Device::Dust(power, Some(shape)).state(&self.registry);
        let changed_ports = current.geometry().ports ^ shape.geometry().ports;
        self.dimension.edit_block_with_notifications(
            position,
            block,
            revision,
            EditNotification::WireToggle(changed_ports),
        )?;
        Ok(Some(Effect::ChunkChanged(position.chunk())))
    }

    pub(crate) fn update_java_wire_shape(
        &mut self,
        position: BlockPos,
        from: Face,
        power: u8,
        shape: Option<Shape>,
    ) {
        if from == Face::Down {
            return;
        }
        let current = shape.unwrap_or(Shape::CROSS);
        let next = if from == Face::Up || shape.is_none() {
            self.java_wire_shape(position, current)
        } else {
            let direction = SIDES.iter().position(|&side| side == from).unwrap();
            let Some(connection) =
                self.wire_connection(position, direction, self.wire_climb_open(position))
            else {
                return;
            };
            if connection.connected() == current.0[direction].connected() && !current.is_cross() {
                let mut next = current;
                next.0[direction] = connection;
                next
            } else {
                self.java_wire_shape(position, Shape::CROSS)
            }
        };
        if Some(next) != shape {
            self.write_java_device_state(
                position,
                Device::Dust(power, Some(next)),
                EditNotification::ShapesOnly,
            );
        }
    }

    pub(crate) fn incoming_java_wire(&self, position: BlockPos) -> Option<(BlockPos, u8)> {
        let mut strongest = self.java_wire_external_input(position);
        if strongest.is_some_and(|(_, power)| power == 15) {
            return strongest;
        }
        let climb = self.wire_climb_open(position);
        for face in SIDES {
            let Some(side) = face.offset(position) else {
                continue;
            };
            let Some(block) = self.wire_block_at(side) else {
                continue;
            };
            let stepped = if self.java_conductor(block) {
                climb.then(|| Face::Up.offset(side)).flatten()
            } else {
                Face::Down.offset(side)
            };
            for neighbor in [Some(side), stepped].into_iter().flatten() {
                let power = self.wire_at(neighbor).unwrap_or_default().saturating_sub(1);
                if power > strongest.map_or(0, |(_, strength)| strength) {
                    strongest = Some((neighbor, power));
                }
            }
        }
        strongest
    }
}
