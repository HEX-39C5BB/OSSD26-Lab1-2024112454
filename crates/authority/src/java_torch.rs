use std::collections::{BTreeMap, VecDeque};

use rustcraft_world::{BlockPos, BlockStateId};

use crate::Server;
use crate::java_block_ticks::{BlockTickPriority, ScheduledBlockTick};
use crate::java_redstone::JavaBlockTickKind;
use crate::java_signal::{Face, Signal};
use crate::redstone::Device;
use crate::time::java_redstone_delay;

const BURNOUT_TOGGLES: usize = 8;
const BURNOUT_WINDOW_GT: u64 = 60;
const RECOVERY_DELAY_GT: u64 = 160;

#[derive(Default)]
pub(crate) struct TorchToggles {
    recent: VecDeque<(u64, BlockPos)>,
    counts: BTreeMap<BlockPos, usize>,
}

impl TorchToggles {
    fn expire(&mut self, gt: u64) {
        while let Some(&(at, position)) = self.recent.front() {
            if gt - at <= BURNOUT_WINDOW_GT {
                break;
            }
            self.recent.pop_front();
            let count = self
                .counts
                .get_mut(&position)
                .expect("known torch position");
            *count -= 1;
            if *count == 0 {
                self.counts.remove(&position);
            }
        }
    }

    fn record_off(&mut self, position: BlockPos, gt: u64) -> bool {
        self.recent.push_back((gt, position));
        *self.counts.entry(position).or_default() += 1;
        self.is_burned_out(position)
    }

    fn is_burned_out(&self, position: BlockPos) -> bool {
        self.counts.get(&position).copied().unwrap_or_default() >= BURNOUT_TOGGLES
    }
}

impl Server {
    pub(crate) fn placed_java_torch(
        &self,
        position: BlockPos,
        clicked: BlockPos,
        yaw: f64,
        pitch: f64,
    ) -> Option<BlockStateId> {
        let support = self.preferred_java_attachment(position, clicked, yaw, pitch, false)?;
        Some(Device::Torch { support, lit: true }.state(&self.registry))
    }

    pub(crate) fn update_java_torch(&mut self, position: BlockPos, support: Face, lit: bool) {
        let kind = torch_kind(support);
        let logic = self.java_redstone.as_ref().expect("Java redstone rule");
        if !logic.draining_block_ticks.contains(&(position, kind))
            && !logic.block_ticks.has_scheduled(position, kind)
            && self.java_torch_input(position, support) == Some(lit)
        {
            self.schedule_java_block_tick(
                position,
                kind,
                BlockTickPriority::Normal,
                java_redstone_delay(1),
            );
        }
    }

    pub(crate) fn run_java_torch(&mut self, tick: ScheduledBlockTick<JavaBlockTickKind>) {
        let position = tick.position;
        let Some(Device::Torch { support, lit }) = self.read_device(position) else {
            return;
        };
        if torch_kind(support) != tick.kind {
            return;
        }
        let Some(powered) = self.java_torch_input(position, support) else {
            self.java_redstone
                .as_mut()
                .unwrap()
                .block_ticks
                .schedule(tick);
            return;
        };
        let history = &mut self.java_redstone.as_mut().unwrap().torch_toggles;
        history.expire(self.tick);
        if lit && powered {
            self.write_redstone(
                position,
                Device::Torch {
                    support,
                    lit: false,
                }
                .state(&self.registry),
            );
            self.dispatch_java_edits();
            let history = &mut self.java_redstone.as_mut().unwrap().torch_toggles;
            if history.record_off(position, self.tick) {
                self.schedule_java_block_tick(
                    position,
                    tick.kind,
                    BlockTickPriority::Normal,
                    RECOVERY_DELAY_GT,
                );
            }
        } else if !lit && !powered && !history.is_burned_out(position) {
            self.write_redstone(
                position,
                Device::Torch { support, lit: true }.state(&self.registry),
            );
        }
    }

    fn java_torch_input(&self, position: BlockPos, support: Face) -> Option<bool> {
        let source = support.offset(position)?;
        let signal = self
            .java_power_output(source, support.opposite())?
            .emitted()?;
        Some(signal != Signal::OFF)
    }
}

fn torch_kind(support: Face) -> JavaBlockTickKind {
    if support == Face::Down {
        JavaBlockTickKind::RedstoneTorch
    } else {
        JavaBlockTickKind::RedstoneWallTorch
    }
}
