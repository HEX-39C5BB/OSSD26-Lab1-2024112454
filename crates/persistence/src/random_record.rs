use rustcraft_authority::{RandomScope, RandomStreamKey};
use rustcraft_world::ChunkPos;
use serde::{Deserialize, Serialize};

pub(crate) const RANDOM_STREAMS_SCHEMA: u64 = 1;

#[derive(Serialize, Deserialize)]
pub(crate) struct RandomStreamsRecord {
    pub(crate) schema: u64,
    streams: Vec<RandomStreamRecord>,
}

#[derive(Serialize, Deserialize)]
struct RandomStreamRecord {
    scope: ScopeRecord,
    system: String,
    state: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ScopeRecord {
    World,
    Dimension { dimension: String },
    Chunk { dimension: String, x: i64, z: i64 },
}

impl From<Vec<(RandomStreamKey, u64)>> for RandomStreamsRecord {
    fn from(entries: Vec<(RandomStreamKey, u64)>) -> Self {
        let streams = entries
            .into_iter()
            .map(|(key, state)| {
                let scope = match key.scope {
                    RandomScope::World => ScopeRecord::World,
                    RandomScope::Dimension(dimension) => ScopeRecord::Dimension { dimension },
                    RandomScope::Chunk {
                        dimension,
                        position: ChunkPos { x, z },
                    } => ScopeRecord::Chunk { dimension, x, z },
                };
                RandomStreamRecord {
                    scope,
                    system: key.system,
                    state,
                }
            })
            .collect();
        Self {
            schema: RANDOM_STREAMS_SCHEMA,
            streams,
        }
    }
}

impl From<RandomStreamsRecord> for Vec<(RandomStreamKey, u64)> {
    fn from(record: RandomStreamsRecord) -> Self {
        record
            .streams
            .into_iter()
            .map(|entry| {
                let scope = match entry.scope {
                    ScopeRecord::World => RandomScope::World,
                    ScopeRecord::Dimension { dimension } => RandomScope::Dimension(dimension),
                    ScopeRecord::Chunk { dimension, x, z } => RandomScope::Chunk {
                        dimension,
                        position: ChunkPos { x, z },
                    },
                };
                (
                    RandomStreamKey {
                        scope,
                        system: entry.system,
                    },
                    entry.state,
                )
            })
            .collect()
    }
}
