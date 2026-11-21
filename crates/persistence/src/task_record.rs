use std::io;

use rustcraft_authority::ScheduledTask;
use rustcraft_authority::checkpoint::{SavedBlock, SavedScheduledOperation, SavedScheduledTasks};
use rustcraft_world::{BlockPos, ChunkPos};
use serde::{Deserialize, Serialize};

use crate::{invalid, records};

const LEGACY_SCHEMA: u64 = 1;
const SCHEMA: u64 = 2;

#[derive(Serialize, Deserialize)]
pub(crate) struct ScheduledTasksRecord {
    schema: u64,
    next_id: u64,
    pending: Vec<TaskRecord>,
}

#[derive(Serialize, Deserialize)]
struct TaskRecord {
    due_tick: u64,
    id: u64,
    operation: Option<OperationRecord>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum OperationRecord {
    Event,
    ReplaceBlock {
        position: [i64; 3],
        expected: BlockRecord,
        replacement: BlockRecord,
    },
    DeactivateChunk {
        position: [i64; 2],
    },
    RedstoneCheck {
        position: [i64; 3],
    },
}

#[derive(Serialize, Deserialize)]
struct BlockRecord {
    name: String,
    version: u32,
}

impl ScheduledTasksRecord {
    pub(crate) fn validate(&self) -> io::Result<()> {
        if self.schema == LEGACY_SCHEMA {
            if self.pending.iter().any(|task| task.operation.is_some()) {
                return Err(invalid(
                    "legacy scheduled tasks must not declare an operation",
                ));
            }
        } else {
            records::check_version(self.schema, SCHEMA, "scheduled tasks")?;
            if self.pending.iter().any(|task| task.operation.is_none()) {
                return Err(invalid("missing scheduled task operation"));
            }
        }
        Ok(())
    }
}

impl From<SavedScheduledTasks> for ScheduledTasksRecord {
    fn from(saved: SavedScheduledTasks) -> Self {
        Self {
            schema: SCHEMA,
            next_id: saved.next_id,
            pending: saved
                .pending
                .into_iter()
                .map(|(task, operation)| TaskRecord {
                    due_tick: task.due_tick,
                    id: task.id,
                    operation: Some(operation.into()),
                })
                .collect(),
        }
    }
}

impl From<ScheduledTasksRecord> for SavedScheduledTasks {
    fn from(saved: ScheduledTasksRecord) -> Self {
        Self {
            next_id: saved.next_id,
            pending: saved
                .pending
                .into_iter()
                .map(|task| {
                    (
                        ScheduledTask {
                            due_tick: task.due_tick,
                            id: task.id,
                        },
                        task.operation.unwrap_or(OperationRecord::Event).into(),
                    )
                })
                .collect(),
        }
    }
}

impl From<SavedScheduledOperation> for OperationRecord {
    fn from(operation: SavedScheduledOperation) -> Self {
        match operation {
            SavedScheduledOperation::Event => Self::Event,
            SavedScheduledOperation::RedstoneCheck { position } => Self::RedstoneCheck {
                position: [position.x, position.y, position.z],
            },
            SavedScheduledOperation::DeactivateChunk { position } => Self::DeactivateChunk {
                position: [position.x, position.z],
            },
            SavedScheduledOperation::ReplaceBlock {
                position,
                expected,
                replacement,
            } => Self::ReplaceBlock {
                position: [position.x, position.y, position.z],
                expected: BlockRecord {
                    name: expected.name,
                    version: expected.version,
                },
                replacement: BlockRecord {
                    name: replacement.name,
                    version: replacement.version,
                },
            },
        }
    }
}

impl From<OperationRecord> for SavedScheduledOperation {
    fn from(operation: OperationRecord) -> Self {
        match operation {
            OperationRecord::RedstoneCheck {
                position: [x, y, z],
            } => Self::RedstoneCheck {
                position: BlockPos::new(x, y, z),
            },
            OperationRecord::Event => Self::Event,
            OperationRecord::DeactivateChunk { position: [x, z] } => Self::DeactivateChunk {
                position: ChunkPos { x, z },
            },
            OperationRecord::ReplaceBlock {
                position: [x, y, z],
                expected,
                replacement,
            } => Self::ReplaceBlock {
                position: BlockPos::new(x, y, z),
                expected: SavedBlock {
                    name: expected.name,
                    version: expected.version,
                },
                replacement: SavedBlock {
                    name: replacement.name,
                    version: replacement.version,
                },
            },
        }
    }
}
