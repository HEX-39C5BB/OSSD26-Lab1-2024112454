use rustcraft_world::BlockPos;

use crate::java_block_ticks::{BlockTickPriority, ScheduledBlockTick};
use crate::java_redstone::{EditNotification, JavaBlockTickKind};
use crate::java_signal::{DEVICE_FACES, Face, Signal};
use crate::redstone::{Comparator, Device};
use crate::time::java_redstone_delay;
use crate::{Effect, Rejection, Server};

const KIND: JavaBlockTickKind = JavaBlockTickKind::Comparator;

impl Server {
    pub(crate) fn initialize_placed_comparator(
        &mut self,
        position: BlockPos,
        comparator: Comparator,
    ) {
        if self
            .evaluate_java_comparator(position, comparator)
            .is_some_and(|(_, powered)| powered)
        {
            self.schedule_java_block_tick(position, KIND, BlockTickPriority::Normal, 1);
        }
    }

    pub(crate) fn update_java_comparator(&mut self, position: BlockPos, comparator: Comparator) {
        let logic = self.java_redstone.as_ref().unwrap();
        if logic.draining_block_ticks.contains(&(position, KIND))
            || logic.block_ticks.has_scheduled(position, KIND)
        {
            return;
        }
        let Some((signal, powered)) = self.evaluate_java_comparator(position, comparator) else {
            return;
        };
        if signal != self.java_comparator_strength(position) || powered != comparator.powered {
            let priority =
                if self.java_diode_has_priority(position, DEVICE_FACES[comparator.output]) {
                    BlockTickPriority::High
                } else {
                    BlockTickPriority::Normal
                };
            self.schedule_java_block_tick(position, KIND, priority, java_redstone_delay(1));
        }
    }

    pub(crate) fn run_java_comparator(&mut self, tick: ScheduledBlockTick<JavaBlockTickKind>) {
        let position = tick.position;
        let Some(Device::Comparator(comparator)) = self.read_device(position) else {
            return;
        };
        if !self.refresh_java_comparator(position, comparator) {
            self.java_redstone
                .as_mut()
                .unwrap()
                .block_ticks
                .schedule(tick);
        }
    }

    pub(crate) fn toggle_java_comparator(
        &mut self,
        position: BlockPos,
        revision: u64,
        mut comparator: Comparator,
    ) -> Result<Option<Effect>, Rejection> {
        comparator.subtract = !comparator.subtract;
        let state = Device::Comparator(comparator).state(&self.registry);
        self.dimension.edit_block_with_notifications(
            position,
            state,
            revision,
            EditNotification::ShapesOnly,
        )?;
        self.dispatch_java_edits();
        self.refresh_java_comparator(position, comparator);
        Ok(Some(Effect::ChunkChanged(position.chunk())))
    }

    fn refresh_java_comparator(&mut self, position: BlockPos, mut comparator: Comparator) -> bool {
        let Some((signal, powered)) = self.evaluate_java_comparator(position, comparator) else {
            return false;
        };
        let previous = self.java_comparator_strength(position);
        let outputs = &mut self.java_redstone.as_mut().unwrap().comparator_outputs;
        if signal == Signal::OFF {
            outputs.remove(&position);
        } else {
            outputs.insert(position, signal);
        }
        if previous != signal || !comparator.subtract {
            if powered != comparator.powered {
                comparator.powered = powered;
                self.write_java_device_state(
                    position,
                    Device::Comparator(comparator),
                    EditNotification::ShapesOnly,
                );
                self.dispatch_java_edits();
            }
            self.notify_java_output(position, DEVICE_FACES[comparator.output]);
        }
        true
    }

    pub(crate) fn java_comparator_strength(&self, position: BlockPos) -> Signal {
        self.java_redstone
            .as_ref()
            .and_then(|logic| logic.comparator_outputs.get(&position).copied())
            .unwrap_or(Signal::OFF)
    }

    fn evaluate_java_comparator(
        &self,
        position: BlockPos,
        comparator: Comparator,
    ) -> Option<(Signal, bool)> {
        let output = DEVICE_FACES[comparator.output];
        let rear = self.java_comparator_rear(position, output)?;
        let side_a = self.java_comparator_side(position, DEVICE_FACES[(comparator.output + 1) % 4]);
        let side_b = self.java_comparator_side(position, DEVICE_FACES[(comparator.output + 3) % 4]);
        let side = if side_a == Some(Signal::MAX) || side_b == Some(Signal::MAX) {
            Signal::MAX
        } else {
            side_a?.max(side_b?)
        };
        let powered = rear > side || (rear == side && rear != Signal::OFF && !comparator.subtract);
        let output = if comparator.subtract {
            Signal::new(rear.get().saturating_sub(side.get())).unwrap()
        } else if powered {
            rear
        } else {
            Signal::OFF
        };
        Some((output, powered))
    }

    fn java_comparator_rear(&self, position: BlockPos, output: Face) -> Option<Signal> {
        let rear = output.opposite();
        let source = rear.offset(position)?;
        let device = self.read_device(source);
        if let Some(signal) = self.java_analog_output(source, device) {
            return Some(signal);
        }
        let power = self.java_power_output(source, output)?.emitted()?;
        let power = if let Some(Device::Dust(strength, _)) = device {
            power.max(Signal::new(strength).unwrap())
        } else {
            power
        };
        if power != Signal::MAX && self.java_signal_cell(source)?.conductor {
            let farther = rear.offset(source)?;
            let (state, _) = self.dimension.voxel(farther)?;
            if let Some(analog) = self.java_analog_output(farther, self.device(state)) {
                return Some(analog);
            }
        }
        Some(power)
    }

    fn java_analog_output(&self, source: BlockPos, device: Option<Device>) -> Option<Signal> {
        match device? {
            Device::Ejector(_) => Some(self.java_container_signal(source)),
            device => device.analog_output(),
        }
    }

    fn java_comparator_side(&self, position: BlockPos, face: Face) -> Option<Signal> {
        let source = face.offset(position)?;
        match self.read_device(source) {
            Some(Device::PowerBlock) => Some(Signal::MAX),
            Some(Device::Dust(strength, _)) => Signal::new(strength),
            _ => Some(self.java_power_output(source, face.opposite())?.direct),
        }
    }

    pub(crate) fn java_diode_has_priority(&self, position: BlockPos, output: Face) -> bool {
        output
            .offset(position)
            .and_then(|front| self.read_device(front))
            .and_then(Device::diode_output)
            .is_some_and(|next| next != output.opposite())
    }

    pub(crate) fn notify_java_comparator_output(
        &mut self,
        source: BlockPos,
    ) -> Result<Effect, Rejection> {
        if self.java_redstone.is_none() {
            return Err(Rejection::MissingCapability);
        }
        if self.dimension.voxel(source).is_none() {
            return Err(Rejection::InactiveChunk);
        }
        for face in &DEVICE_FACES[..4] {
            let Some(neighbor) = face.offset(source) else {
                continue;
            };
            if let Some(Device::Comparator(comparator)) = self.read_device(neighbor) {
                self.update_java_comparator(neighbor, comparator);
            } else if self
                .java_signal_cell(neighbor)
                .is_some_and(|cell| cell.conductor)
                && let Some(farther) = face.offset(neighbor)
                && let Some(Device::Comparator(comparator)) = self.read_device(farther)
            {
                self.update_java_comparator(farther, comparator);
            }
        }
        Ok(Effect::Unchanged)
    }
}
