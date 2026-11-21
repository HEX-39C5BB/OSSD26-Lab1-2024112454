use std::collections::BTreeMap;

use rustcraft_world::ChunkPos;

use crate::{RuleVersion, SeedScope, Server};

pub(crate) const MAX_STREAMS: usize = 256;
const MAX_ID_BYTES: usize = 128;

/// The stable scope of a system stream in a saved world.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RandomScope {
    World,
    Dimension(String),
    Chunk {
        dimension: String,
        position: ChunkPos,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RandomStreamKey {
    pub scope: RandomScope,
    pub system: String,
}

impl RandomStreamKey {
    pub(crate) fn is_valid(&self) -> bool {
        let valid = |id: &str| (1..=MAX_ID_BYTES).contains(&id.len());
        valid(&self.system)
            && match &self.scope {
                RandomScope::World => true,
                RandomScope::Dimension(id) | RandomScope::Chunk { dimension: id, .. } => valid(id),
            }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum RandomAlgorithm {
    NativeV1,
}

impl RandomAlgorithm {
    pub(crate) const fn identity(self) -> RuleVersion {
        match self {
            Self::NativeV1 => RuleVersion {
                id: "rustcraft:random_stream",
                version: 1,
            },
        }
    }

    fn draw(self, state: &mut u64) -> u64 {
        match self {
            Self::NativeV1 => {
                *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
                crate::seed::mix_v1(*state)
            }
        }
    }
}

/// Staged draws from one authoritative stream. Dropping them rolls back the draws.
#[must_use = "commit the stream to retain its draws"]
pub struct SystemRandom<'a> {
    streams: &'a mut BTreeMap<RandomStreamKey, u64>,
    key: RandomStreamKey,
    algorithm: RandomAlgorithm,
    start: u64,
    state: u64,
}

impl SystemRandom<'_> {
    pub fn next_u64(&mut self) -> u64 {
        self.algorithm.draw(&mut self.state)
    }

    pub fn commit(self) {
        if self.state != self.start {
            self.streams.insert(self.key, self.state);
        }
    }
}

impl Server {
    /// Begin a random stream transaction for a trusted authority system.
    pub fn system_random(
        &mut self,
        scope: SeedScope<'_>,
        system: &str,
    ) -> Result<SystemRandom<'_>, &'static str> {
        let key = RandomStreamKey {
            scope: match scope {
                SeedScope::World => RandomScope::World,
                SeedScope::Dimension(id) => RandomScope::Dimension(id.into()),
                SeedScope::Chunk {
                    dimension,
                    position,
                } => RandomScope::Chunk {
                    dimension: dimension.into(),
                    position,
                },
            },
            system: system.into(),
        };
        if !key.is_valid() {
            return Err("invalid random stream identifier");
        }
        let state = match self.random_streams.get(&key) {
            Some(&state) => state,
            None if self.random_streams.len() == MAX_STREAMS => {
                return Err("random stream limit reached");
            }
            None => self.world_seeds().for_system(scope, system),
        };
        Ok(SystemRandom {
            streams: &mut self.random_streams,
            key,
            algorithm: self.rules.random_algorithm(),
            start: state,
            state,
        })
    }
}
