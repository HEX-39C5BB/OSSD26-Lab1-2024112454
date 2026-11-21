use rustcraft_world::{BlockPos, BlockStateId};

use crate::Server;
use crate::java_block_ticks::BlockTickPriority;
use crate::java_redstone::{EditNotification, JavaBlockTickKind};
use crate::java_signal::DEVICE_FACES;
use crate::redstone::Device;
use crate::time::java_redstone_delay;

const KIND: JavaBlockTickKind = JavaBlockTickKind::Observer;

impl Server {
    /// Apply the state and initial shape check of a host block-state write.
    pub(crate) fn prepare_observer_write(
        &self,
        position: BlockPos,
        requested: BlockStateId,
    ) -> (BlockStateId, bool) {
        match self.device(requested) {
            Some(Device::Observer(_, false)) => (requested, true),
            Some(Device::Observer(facing, true))
                if !matches!(self.read_device(position), Some(Device::Observer(..)))
                    && !self.observer_tick_scheduled(position) =>
            {
                (Device::Observer(facing, false).state(&self.registry), false)
            }
            _ => (requested, false),
        }
    }

    pub(crate) fn arm_java_observer(&mut self, position: BlockPos) {
        if !self.observer_tick_scheduled(position) {
            self.schedule_java_block_tick(
                position,
                KIND,
                BlockTickPriority::Normal,
                java_redstone_delay(1),
            );
        }
    }

    pub(crate) fn run_java_observer(&mut self, position: BlockPos) {
        let Some(Device::Observer(facing, powered)) = self.read_device(position) else {
            return;
        };
        let output = DEVICE_FACES[facing].opposite();
        self.write_java_device_state(
            position,
            Device::Observer(facing, !powered),
            EditNotification::ShapesOnly,
        );
        // Shape callbacks can arm another observer before this pulse queues its falling edge.
        self.dispatch_java_edits();
        if !powered {
            self.schedule_java_block_tick(
                position,
                KIND,
                BlockTickPriority::Normal,
                java_redstone_delay(1),
            );
        }
        self.notify_java_output(position, output);
    }

    fn observer_tick_scheduled(&self, position: BlockPos) -> bool {
        self.java_redstone
            .as_ref()
            .is_some_and(|logic| logic.block_ticks.has_scheduled(position, KIND))
    }
}
