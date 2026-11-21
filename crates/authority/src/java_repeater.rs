use rustcraft_world::BlockPos;

use crate::Server;
use crate::java_block_ticks::{BlockTickPriority, ScheduledBlockTick};
use crate::java_redstone::{EditNotification, JavaBlockTickKind};
use crate::java_signal::{DEVICE_FACES, Face, PowerContext, Signal, power_output};
use crate::redstone::{Device, Repeater};
use crate::time::java_redstone_delay;

const KIND: JavaBlockTickKind = JavaBlockTickKind::Repeater;

impl Server {
    pub(crate) fn initialize_placed_diode(&mut self, position: BlockPos) {
        if self.java_redstone.is_none() {
            return;
        }
        match self.read_device(position) {
            Some(Device::Repeater(repeater)) => {
                self.update_java_repeater_shape(position, repeater, repeater_sides(repeater)[0]);
                if self.java_repeater_input(position, repeater) == Some(true) {
                    self.schedule_java_block_tick(position, KIND, BlockTickPriority::Normal, 1);
                }
            }
            Some(Device::Comparator(comparator)) => {
                self.initialize_placed_comparator(position, comparator)
            }
            _ => {}
        }
    }

    pub(crate) fn update_java_repeater(&mut self, position: BlockPos, repeater: Repeater) {
        let logic = self.java_redstone.as_ref().unwrap();
        if logic.draining_block_ticks.contains(&(position, KIND))
            || logic.block_ticks.has_scheduled(position, KIND)
            || self.java_repeater_locked(position, repeater)
            || self.java_repeater_input(position, repeater) != Some(!repeater.powered)
        {
            return;
        }
        let output = DEVICE_FACES[repeater.output];
        let priority = if self.java_diode_has_priority(position, output) {
            BlockTickPriority::ExtremelyHigh
        } else if repeater.powered {
            BlockTickPriority::VeryHigh
        } else {
            BlockTickPriority::High
        };
        self.schedule_java_block_tick(
            position,
            KIND,
            priority,
            java_redstone_delay(repeater.delay_rt),
        );
    }

    pub(crate) fn update_java_repeater_shape(
        &mut self,
        position: BlockPos,
        mut repeater: Repeater,
        from: Face,
    ) {
        if !repeater_sides(repeater).contains(&from) {
            return;
        }
        let locked = self.java_repeater_locked(position, repeater);
        if repeater.locked != locked {
            repeater.locked = locked;
            self.write_java_device_state(
                position,
                Device::Repeater(repeater),
                EditNotification::ShapesOnly,
            );
        }
    }

    pub(crate) fn run_java_repeater(&mut self, tick: ScheduledBlockTick<JavaBlockTickKind>) {
        let position = tick.position;
        let Some(Device::Repeater(mut repeater)) = self.read_device(position) else {
            return;
        };
        if self.java_repeater_locked(position, repeater) {
            return;
        }
        let Some(input) = self.java_repeater_input(position, repeater) else {
            self.java_redstone
                .as_mut()
                .unwrap()
                .block_ticks
                .schedule(tick);
            return;
        };
        if repeater.powered && input {
            return;
        }
        repeater.powered = !repeater.powered;
        self.write_java_device_state(
            position,
            Device::Repeater(repeater),
            EditNotification::DirectionalOutput(DEVICE_FACES[repeater.output]),
        );
        if repeater.powered && !input {
            self.schedule_java_block_tick(
                position,
                KIND,
                BlockTickPriority::VeryHigh,
                java_redstone_delay(repeater.delay_rt),
            );
        }
    }

    fn java_repeater_input(&self, position: BlockPos, repeater: Repeater) -> Option<bool> {
        let output = DEVICE_FACES[repeater.output];
        let Some(source) = output.opposite().offset(position) else {
            return Some(false);
        };
        let power = power_output(source, output, PowerContext::Normal, |pos| {
            self.java_signal_cell(pos)
        })?;
        if power.normal != Signal::OFF
            || matches!(self.read_device(source), Some(Device::Dust(1.., _)))
        {
            Some(true)
        } else {
            power.conducted.map(|signal| signal != Signal::OFF)
        }
    }

    fn java_repeater_locked(&self, position: BlockPos, repeater: Repeater) -> bool {
        repeater_sides(repeater).into_iter().any(|face| {
            let Some(source) = face.offset(position) else {
                return false;
            };
            self.read_device(source).and_then(Device::diode_output) == Some(face.opposite())
                && self
                    .java_power_output(source, face.opposite())
                    .is_some_and(|power| power.direct != Signal::OFF)
        })
    }
}

fn repeater_sides(repeater: Repeater) -> [Face; 2] {
    [
        DEVICE_FACES[(repeater.output + 1) % 4],
        DEVICE_FACES[(repeater.output + 3) % 4],
    ]
}
