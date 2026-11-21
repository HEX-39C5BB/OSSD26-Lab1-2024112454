use std::collections::VecDeque;

use rustcraft_world::ChunkPos;

use crate::{Dimension, Player};

const ACTIVATIONS_PER_TICK: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewDistance(u8);

impl ViewDistance {
    pub const MIN_RADIUS: u8 = 1;
    pub const MAX_RADIUS: u8 = 16;

    pub const fn radius(self) -> u8 {
        self.0
    }

    pub const fn from_radius(radius: u8) -> Option<Self> {
        if radius >= Self::MIN_RADIUS && radius <= Self::MAX_RADIUS {
            Some(Self(radius))
        } else {
            None
        }
    }
}

impl Default for ViewDistance {
    fn default() -> Self {
        Self(2)
    }
}

pub(super) struct ChunkView {
    pub distance: ViewDistance,
    pub center: Option<ChunkPos>,
    pending: VecDeque<ChunkPos>,
}

impl ChunkView {
    pub fn new(distance: ViewDistance) -> Self {
        Self {
            distance,
            center: None,
            pending: VecDeque::new(),
        }
    }

    pub fn refresh(&mut self, dimension: &mut Dimension, player: Player) {
        let center = player.block_position().chunk();
        if self.center != Some(center) {
            self.center = Some(center);
            let radius = u64::from(self.distance.radius());
            let retired: Vec<_> = dimension
                .active_positions()
                .filter(|position| {
                    position
                        .x
                        .abs_diff(center.x)
                        .max(position.z.abs_diff(center.z))
                        > radius
                })
                .collect();
            for position in retired {
                let _ = dimension.set_active_checked(position, false, Some(player));
            }

            let radius = i64::from(self.distance.radius());
            let mut missing: Vec<_> = (-radius..=radius)
                .flat_map(|x| {
                    (-radius..=radius).map(move |z| ChunkPos {
                        x: center.x + x,
                        z: center.z + z,
                    })
                })
                .filter(|&position| !dimension.is_active(position))
                .collect();
            missing.sort_unstable_by_key(|&position| {
                let x = position.x.abs_diff(center.x);
                let z = position.z.abs_diff(center.z);
                (x.max(z), x + z, position)
            });
            self.pending = missing.into();
        }
        for _ in 0..ACTIVATIONS_PER_TICK {
            let Some(position) = self.pending.pop_front() else {
                break;
            };
            dimension.set_active(position, true);
        }
    }
}
