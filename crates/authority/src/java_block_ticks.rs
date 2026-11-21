//! Reference scheduled-block queue for Java 1.20.1. This queue has its own
//! ordering and lifecycle, separate from the RustCraft world operations queue.

use std::collections::{BTreeMap, BTreeSet};

use rustcraft_world::{BlockPos, ChunkPos};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i8)]
pub enum BlockTickPriority {
    ExtremelyHigh = -3,
    VeryHigh = -2,
    High = -1,
    Normal = 0,
    Low = 1,
    VeryLow = 2,
    ExtremelyLow = 3,
}

/// A scheduled callback to a block type, independent of its current state.
/// The level supplies the sub-tick sequence when creating the record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScheduledBlockTick<K> {
    pub due_gt: u64,
    pub priority: BlockTickPriority,
    pub sub_tick_order: u64,
    pub position: BlockPos,
    pub kind: K,
}

struct ChunkQueue<K> {
    scheduled: BTreeSet<ScheduledBlockTick<K>>,
    targets: BTreeSet<(BlockPos, K)>,
}

impl<K> Default for ChunkQueue<K> {
    fn default() -> Self {
        Self {
            scheduled: BTreeSet::new(),
            targets: BTreeSet::new(),
        }
    }
}

pub struct JavaBlockTicks<K> {
    chunks: BTreeMap<ChunkPos, ChunkQueue<K>>,
}

impl<K> Default for JavaBlockTicks<K> {
    fn default() -> Self {
        Self {
            chunks: BTreeMap::new(),
        }
    }
}

impl<K: Copy + Ord> JavaBlockTicks<K> {
    pub fn pending(&self) -> impl Iterator<Item = ScheduledBlockTick<K>> + '_ {
        self.chunks
            .values()
            .flat_map(|queue| queue.scheduled.iter().copied())
    }

    pub(crate) fn has_scheduled(&self, position: BlockPos, kind: K) -> bool {
        self.chunks
            .get(&position.chunk())
            .is_some_and(|queue| queue.targets.contains(&(position, kind)))
    }

    /// Keep the first pending callback for a position and block type.
    pub fn schedule(&mut self, tick: ScheduledBlockTick<K>) -> bool {
        let queue = self.chunks.entry(tick.position.chunk()).or_default();
        if !queue.targets.insert((tick.position, tick.kind)) {
            return false;
        }
        queue.scheduled.insert(tick);
        true
    }

    /// Collect the entire executable batch before invoking any block callback.
    /// The caller owns the ticking-chunk predicate and the execution phase.
    pub fn collect_due(
        &mut self,
        gt: u64,
        limit: usize,
        mut is_ticking: impl FnMut(ChunkPos) -> bool,
    ) -> Vec<ScheduledBlockTick<K>> {
        let ready = |chunk, tick: &ScheduledBlockTick<K>| {
            (tick.due_gt <= gt).then_some((tick.priority, tick.sub_tick_order, chunk))
        };
        let mut candidates = self
            .chunks
            .iter()
            .filter_map(|(&chunk, queue)| {
                let candidate = ready(chunk, queue.scheduled.first()?)?;
                is_ticking(chunk).then_some(candidate)
            })
            .collect::<BTreeSet<_>>();

        let mut batch = Vec::new();
        while batch.len() < limit {
            let Some((_, _, chunk)) = candidates.pop_first() else {
                break;
            };
            let queue = self.chunks.get_mut(&chunk).expect("candidate chunk");
            let tick = queue.scheduled.pop_first().expect("candidate callback");
            queue.targets.remove(&(tick.position, tick.kind));
            batch.push(tick);
            if let Some(next) = queue.scheduled.first() {
                if let Some(candidate) = ready(chunk, next) {
                    candidates.insert(candidate);
                }
            } else {
                self.chunks.remove(&chunk);
            }
        }
        batch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(
        kind: &str,
        x: i64,
        due_gt: u64,
        priority: BlockTickPriority,
        sub_tick_order: u64,
    ) -> ScheduledBlockTick<&str> {
        ScheduledBlockTick {
            due_gt,
            priority,
            sub_tick_order,
            position: BlockPos::new(x, 6, 4),
            kind,
        }
    }

    #[test]
    fn orders_ready_chunk_heads_by_priority_then_sub_tick_order() {
        use BlockTickPriority::{ExtremelyHigh, High, Normal};
        let mut queue = JavaBlockTicks::default();
        for tick in [
            tick("older_normal", 4, 1, Normal, 0),
            tick("urgent_later", 1, 2, ExtremelyHigh, 1),
            tick("other_high", 16, 1, High, 2),
            tick("other_normal", 2, 1, Normal, 3),
        ] {
            assert!(queue.schedule(tick));
        }
        assert!(!queue.schedule(tick("older_normal", 4, 1, ExtremelyHigh, 4)));
        let order: Vec<_> = queue
            .collect_due(2, 10, |_| true)
            .into_iter()
            .map(|ready| ready.kind)
            .collect();
        // Observed from Java 1.20.1 LevelTicks in the ORDER-CHUNK fixture.
        assert_eq!(
            order,
            ["other_high", "older_normal", "other_normal", "urgent_later"]
        );
        assert!(queue.collect_due(2, 10, |_| true).is_empty());
    }

    #[test]
    fn preserves_due_records_when_a_chunk_is_inactive_or_the_batch_is_full() {
        use BlockTickPriority::{ExtremelyHigh, Normal};
        let mut queue = JavaBlockTicks::default();
        let earlier = tick("first", 3, 4, Normal, 0);
        let blocked = tick("neighbor", -1, 4, ExtremelyHigh, 1);
        let distinct_type = tick("different_kind", 3, 4, Normal, 2);
        let future = tick("later", 4, 8, Normal, 3);
        for tick in [earlier, blocked, distinct_type, future] {
            assert!(queue.schedule(tick));
        }
        assert!(queue.collect_due(3, 10, |_| true).is_empty());
        assert_eq!(queue.collect_due(4, 1, |chunk| chunk.x >= 0), [earlier]);
        assert_eq!(queue.collect_due(5, 10, |_| true), [blocked, distinct_type]);
        assert_eq!(queue.collect_due(8, 10, |_| true), [future]);
        assert!(queue.schedule(earlier));
    }
}
