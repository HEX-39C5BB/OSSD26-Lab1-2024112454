use bevy_ecs::prelude::Component;
use rustcraft_world::BlockPos;

use crate::dimension::Dimension;
use crate::player::Bounds;
use crate::{ItemStack, Server};

const PICKUP_AGE: u16 = 10;
pub(crate) const LIFETIME_GT: u16 = 6_000;

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct DroppedItem {
    pub stack: ItemStack,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub age: u16,
}

impl DroppedItem {
    pub(crate) fn block_position(self) -> BlockPos {
        let [x, y, z] = self.position.map(|axis| axis.floor() as i64);
        BlockPos::new(x, y, z)
    }

    pub(crate) fn bounds(self) -> Bounds {
        let [x, y, z] = self.position;
        Bounds {
            min: [x - 0.125, y, z - 0.125],
            max: [x + 0.125, y + 0.25, z + 0.125],
        }
    }

    fn step(&mut self, dimension: &Dimension) {
        self.velocity[1] -= 0.04;
        let mut grounded = false;
        for axis in [1, 0, 2] {
            let travel = self.velocity[axis];
            let allowed = self.bounds().clip_motion(dimension, axis, travel);
            self.position[axis] += allowed;
            if travel != allowed {
                if axis == 1 {
                    grounded = travel < 0.0;
                    self.velocity[axis] *= -0.5;
                } else {
                    self.velocity[axis] = 0.0;
                }
            }
        }
        let friction = if grounded { 0.588 } else { 0.98 };
        self.velocity[0] *= friction;
        self.velocity[1] *= 0.98;
        self.velocity[2] *= friction;
        self.age += 1;
    }
}

impl Server {
    pub(crate) fn spawn_java_item(
        &mut self,
        stack: ItemStack,
        position: [f64; 3],
        velocity: [f64; 3],
    ) {
        let id = self.allocate_entity();
        self.dimension.entities.spawn_item(
            id,
            DroppedItem {
                stack,
                position,
                velocity,
                age: 0,
            },
        );
    }

    pub(crate) fn tick_java_items(&mut self) {
        let items: Vec<_> = self.dimension.entities.items().collect();
        for (id, mut item) in items {
            if !self.dimension.is_active(item.block_position().chunk()) {
                continue;
            }
            item.step(&self.dimension);
            if item.age >= LIFETIME_GT || self.pick_up_java_item(&mut item) {
                self.dimension.entities.set_item(id, None);
            } else {
                self.dimension.entities.set_item(id, Some(item));
                self.touch_java_item_pressure_plates(item.bounds());
            }
        }
    }

    fn pick_up_java_item(&mut self, item: &mut DroppedItem) -> bool {
        if item.age < PICKUP_AGE {
            return false;
        }
        let Some((id, player)) = self.player_body() else {
            return false;
        };
        let player = player.bounds();
        let body = item.bounds();
        if !(0..3).all(|axis| {
            let distance = if axis == 1 { 0.0 } else { 0.5 };
            body.max[axis] > player.min[axis] - distance
                && body.min[axis] < player.max[axis] + distance
        }) {
            return false;
        }
        let mut inventory = self.dimension.entities.inventory(id).unwrap().clone();
        let before = item.stack.count;
        while item.stack.count != 0 && inventory.insert_v1(item.stack.block).is_ok() {
            item.stack.count -= 1;
        }
        if before != item.stack.count {
            self.dimension.entities.set_inventory(id, inventory);
        }
        item.stack.count == 0
    }
}
