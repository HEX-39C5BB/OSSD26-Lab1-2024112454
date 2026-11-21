use std::collections::{BTreeMap, BTreeSet};

use rustcraft_world::{BlockPos, BlockStateId, ChunkPos};

use crate::{Effect, Rejection, Server};

const MAX_PENDING_TASKS: usize = 4096;

/// Handle returned at scheduling and published again when the task comes due.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScheduledTask {
    pub due_tick: u64,
    pub id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduledOperation {
    Event,
    ReplaceBlock {
        position: BlockPos,
        expected: BlockStateId,
        replacement: BlockStateId,
    },
    DeactivateChunk {
        position: ChunkPos,
    },
    RedstoneCheck {
        position: BlockPos,
    },
}

impl ScheduledOperation {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::ReplaceBlock { .. } => "replace_block",
            Self::DeactivateChunk { .. } => "deactivate_chunk",
            Self::RedstoneCheck { .. } => "redstone_check",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskEvent {
    Queued {
        task: ScheduledTask,
        operation: ScheduledOperation,
    },
    Canceled {
        task: ScheduledTask,
        operation: ScheduledOperation,
    },
    Due {
        task: ScheduledTask,
        operation: ScheduledOperation,
        result: Result<Effect, Rejection>,
    },
}

pub(crate) struct TaskQueue {
    pub(crate) next_id: u64,
    pub(crate) pending: BTreeMap<ScheduledTask, ScheduledOperation>,
    pub(crate) events: Vec<TaskEvent>,
}

impl Default for TaskQueue {
    fn default() -> Self {
        Self {
            next_id: 1,
            pending: BTreeMap::new(),
            events: Vec::new(),
        }
    }
}

impl TaskQueue {
    pub(crate) fn restore(
        tick: u64,
        next_id: u64,
        pending: Vec<(ScheduledTask, ScheduledOperation)>,
    ) -> Result<Self, String> {
        if next_id == 0 || pending.len() > MAX_PENDING_TASKS {
            return Err("invalid saved task counter or queue length".into());
        }
        let mut ids = BTreeSet::new();
        for (task, _) in &pending {
            if task.id == 0
                || task.id >= next_id
                || task.due_tick <= tick
                || task.due_tick == u64::MAX
                || !ids.insert(task.id)
            {
                return Err("invalid or duplicate saved scheduled task".into());
            }
        }
        Ok(Self {
            next_id,
            pending: pending.into_iter().collect(),
            events: Vec::new(),
        })
    }

    pub(crate) fn pop_due(&mut self, tick: u64) -> Option<(ScheduledTask, ScheduledOperation)> {
        self.pending
            .first_key_value()
            .filter(|(task, _)| task.due_tick <= tick)?;
        self.pending.pop_first()
    }
}

impl Server {
    /// Iterate over pending operations by due tick and task ID.
    pub fn scheduled_tasks(
        &self,
    ) -> impl ExactSizeIterator<Item = (ScheduledTask, ScheduledOperation)> + '_ {
        self.tasks
            .pending
            .iter()
            .map(|(&task, &operation)| (task, operation))
    }

    /// Schedule a world-level due event from a trusted authority system.
    pub fn schedule_task(&mut self, delay_ticks: u64) -> Result<ScheduledTask, &'static str> {
        self.schedule_operation(delay_ticks, ScheduledOperation::Event)
    }

    /// Queue an operation from a trusted authority system.
    pub fn schedule_operation(
        &mut self,
        delay_ticks: u64,
        operation: ScheduledOperation,
    ) -> Result<ScheduledTask, &'static str> {
        self.validate_scheduled_operation(operation)?;
        let due_tick = self
            .tick
            .checked_add(delay_ticks)
            .filter(|&due| due > self.tick && due < u64::MAX)
            .ok_or("scheduled task must have a reachable future tick")?;
        if self.tasks.pending.len() == MAX_PENDING_TASKS {
            return Err("scheduled task queue is full");
        }
        let id = self.tasks.next_id;
        self.tasks.next_id = id
            .checked_add(1)
            .ok_or("scheduled task identity exhausted")?;
        let task = ScheduledTask { due_tick, id };
        self.tasks.pending.insert(task, operation);
        self.tasks
            .events
            .push(TaskEvent::Queued { task, operation });
        Ok(task)
    }

    /// Cancel a pending task identified by its original handle.
    pub fn cancel_task(&mut self, task: ScheduledTask) -> bool {
        let Some(operation) = self.tasks.pending.remove(&task) else {
            return false;
        };
        self.tasks
            .events
            .push(TaskEvent::Canceled { task, operation });
        true
    }

    pub(crate) fn validate_scheduled_operation(
        &self,
        operation: ScheduledOperation,
    ) -> Result<(), &'static str> {
        if let ScheduledOperation::RedstoneCheck { position } = operation
            && (!self.rules().is_java_flat()
                || self
                    .dimension
                    .height
                    .is_none_or(|height| !height.contains(position.y)))
        {
            return Err("redstone check requires an in-range Java flat block");
        }
        if let ScheduledOperation::ReplaceBlock {
            position,
            expected,
            replacement,
        } = operation
        {
            if self.registry.block(expected).is_none() || self.registry.block(replacement).is_none()
            {
                return Err("unknown scheduled block state");
            }
            if self
                .dimension
                .height
                .is_some_and(|height| !height.contains(position.y))
            {
                return Err("scheduled block target is outside the world");
            }
        }
        Ok(())
    }

    pub(crate) fn execute_scheduled_operation(
        &mut self,
        operation: ScheduledOperation,
    ) -> Result<Effect, Rejection> {
        match operation {
            ScheduledOperation::Event => Ok(Effect::Unchanged),
            ScheduledOperation::RedstoneCheck { position } => self.run_redstone_check(position),
            ScheduledOperation::DeactivateChunk { position } => {
                self.set_chunk_active(position, false)
            }
            ScheduledOperation::ReplaceBlock {
                position,
                expected,
                replacement,
            } => {
                let (actual, revision) = self
                    .dimension
                    .voxel(position)
                    .ok_or(Rejection::InactiveChunk)?;
                if actual != expected {
                    return Err(Rejection::UnexpectedBlockState { actual });
                }
                self.set_block(position, replacement, revision)
            }
        }
    }
}
