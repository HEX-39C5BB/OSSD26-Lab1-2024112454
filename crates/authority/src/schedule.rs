#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Commit,
    JavaScheduledBlocks,
    JavaBlockEvents,
    SimulatePlayer,
    JavaItemMotion,
    JavaPistonMotion,
    RefreshChunkView,
    Publish,
}

impl Phase {
    fn mask(self) -> u8 {
        1 << self as u8
    }
}

#[derive(Clone, Copy)]
struct System {
    phase: Phase,
    after: &'static [Phase],
    reads: u8,
    writes: u8,
}

const WORLD: u8 = 1;
const ENTITIES: u8 = 2;

const SYSTEMS_V1: &[System] = &[
    System {
        phase: Phase::Commit,
        after: &[],
        reads: WORLD | ENTITIES,
        writes: WORLD | ENTITIES,
    },
    System {
        phase: Phase::SimulatePlayer,
        after: &[Phase::Commit],
        reads: WORLD | ENTITIES,
        writes: ENTITIES,
    },
    System {
        phase: Phase::RefreshChunkView,
        after: &[Phase::SimulatePlayer],
        reads: WORLD | ENTITIES,
        writes: WORLD,
    },
    System {
        phase: Phase::Publish,
        after: &[Phase::RefreshChunkView],
        reads: WORLD | ENTITIES,
        writes: 0,
    },
];

pub(crate) fn build(java_block_events: bool) -> Result<Vec<Phase>, &'static str> {
    let mut systems = SYSTEMS_V1.to_vec();
    if java_block_events {
        systems.extend([
            System {
                phase: Phase::JavaScheduledBlocks,
                after: &[Phase::Commit],
                reads: WORLD | ENTITIES,
                writes: WORLD | ENTITIES,
            },
            System {
                phase: Phase::JavaBlockEvents,
                after: &[Phase::JavaScheduledBlocks],
                reads: WORLD | ENTITIES,
                writes: WORLD,
            },
            System {
                phase: Phase::JavaItemMotion,
                after: &[Phase::SimulatePlayer],
                reads: WORLD | ENTITIES,
                writes: WORLD | ENTITIES,
            },
            System {
                phase: Phase::JavaPistonMotion,
                after: &[Phase::JavaItemMotion],
                reads: WORLD | ENTITIES,
                writes: WORLD,
            },
        ]);
        for system in &mut systems {
            match system.phase {
                Phase::SimulatePlayer => {
                    system.after = &[Phase::JavaBlockEvents];
                    system.writes |= WORLD;
                }
                Phase::RefreshChunkView => system.after = &[Phase::JavaPistonMotion],
                _ => {}
            }
        }
    }
    let mut pending: Vec<_> = systems.iter().collect();
    let mut ordered: Vec<(&System, u8)> = Vec::with_capacity(pending.len());
    while !pending.is_empty() {
        let Some(index) = pending.iter().position(|system| {
            system
                .after
                .iter()
                .all(|phase| ordered.iter().any(|(prior, _)| prior.phase == *phase))
        }) else {
            return Err("invalid tick dependency graph");
        };
        let system = pending.remove(index);
        let ancestors = ordered
            .iter()
            .filter(|(prior, _)| system.after.contains(&prior.phase))
            .fold(0, |mask, (prior, parents)| {
                mask | prior.phase.mask() | parents
            });
        for (prior, _) in &ordered {
            if prior.phase == system.phase {
                return Err("duplicate tick phase");
            }
            let conflict =
                system.writes & (prior.reads | prior.writes) | prior.writes & system.reads;
            if conflict != 0 && ancestors & prior.phase.mask() == 0 {
                return Err("unordered conflicting tick systems");
            }
        }
        ordered.push((system, ancestors));
    }
    Ok(ordered
        .into_iter()
        .map(|(system, _)| system.phase)
        .collect())
}
