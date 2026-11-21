use std::collections::VecDeque;

use rustcraft_authority::{DebugInteraction, Snapshot, TickReport};
use rustcraft_scene::draw::{
    Frame, HUD_ACCENT, HUD_TEXT, HudCorner, append_colored_block_outline, append_signal_arrow,
    colored_hud,
};

const CAPACITY: usize = 256;
const UPDATE_COLOR: [f32; 3] = [0.94, 0.39, 0.93];
const SIGNAL_COLOR: [f32; 3] = [1.0, 0.62, 0.17];
const SIGNAL_OFF_COLOR: [f32; 3] = [0.44, 0.70, 1.0];
const INTERACTION_COLOR: [f32; 3] = [0.18, 1.0, 0.75];
const REJECTED_COLOR: [f32; 3] = [1.0, 0.34, 0.34];
const PISTON_COLOR: [f32; 3] = [0.80, 0.95, 0.30];
const EVENT_ROWS: usize = 10;
pub(super) const MAX_PLANNED: usize = 16;

#[derive(Default)]
pub(super) struct TickReplay {
    history: VecDeque<TickReport>,
    cursor: Option<usize>,
    page: usize,
    pub planned: Vec<DebugInteraction>,
    pub pending_until: Option<u64>,
    pub active: bool,
}

impl TickReplay {
    pub fn record(&mut self, report: &TickReport) {
        if self
            .pending_until
            .is_some_and(|end| report.snapshot.tick >= end)
        {
            self.pending_until = None;
        }
        if self
            .history
            .back()
            .is_some_and(|last| last.snapshot.tick == report.snapshot.tick)
        {
            return;
        }
        if self.history.len() == CAPACITY {
            self.history.pop_front();
            self.cursor = self.cursor.map(|index| index.saturating_sub(1));
        }
        self.history.push_back(report.clone());
    }

    pub fn open(&mut self, active: bool) {
        self.active = active;
        self.cursor = None;
        self.page = 0;
        self.planned.clear();
        self.pending_until = None;
    }

    pub fn previous(&mut self) {
        self.page = 0;
        self.cursor = self
            .cursor
            .map(|index| index.saturating_sub(1))
            .or_else(|| self.history.len().checked_sub(2));
    }

    pub fn next(&mut self) {
        self.page = 0;
        self.cursor = self.cursor.and_then(|index| {
            let next = index + 1;
            (next + 1 < self.history.len()).then_some(next)
        });
    }

    pub fn at_live_tick(&self) -> bool {
        self.cursor.is_none()
    }

    pub fn page(&mut self, previous: bool) {
        self.page = if previous {
            self.page.saturating_sub(1)
        } else {
            let last = self.report().map_or(0, |report| {
                (report.block_updates.len()
                    + report.redstone_updates.len()
                    + report.piston_events.len()
                    + report.debug_interactions.len())
                .saturating_sub(1)
                    / EVENT_ROWS
            });
            (self.page + 1).min(last)
        };
    }

    fn report(&self) -> Option<&TickReport> {
        self.cursor
            .map(|index| &self.history[index])
            .or(self.history.back())
    }

    pub fn snapshot<'a>(&'a self, live: &'a Snapshot) -> &'a Snapshot {
        self.cursor
            .map_or(live, |index| &self.history[index].snapshot)
    }

    pub fn draw(
        &self,
        frame: &mut Frame,
        size: [u32; 2],
        eye: [f64; 3],
        paused: bool,
        names: &[&str],
    ) {
        let mut rows = vec![
            "REDSTONE REPLAY".into(),
            "F3+R OR ESC: EXIT   LEFT/RIGHT: GT HISTORY".into(),
            "ENTER: STEP 1 RT (2 GT)   UP/DOWN: EVENTS".into(),
            "FREE CAM  WASD MOVE  SPACE/SHIFT VERTICAL  MOUSE LOOK".into(),
        ];
        rows.push(format!(
            "LIVE: RIGHT USE/PLACE  LEFT BREAK*  MIDDLE PICK   QUEUED {}/{}",
            self.planned.len(),
            MAX_PLANNED
        ));
        rows.push("* BREAK REQUIRES CREATIVE".into());
        if self.at_live_tick() {
            for interaction in &self.planned {
                append_colored_block_outline(
                    &mut frame.terrain,
                    interaction.position(),
                    eye,
                    INTERACTION_COLOR,
                );
            }
        }
        let mut event_colors = Vec::new();
        if let Some(report) = self.report() {
            let tick = report.snapshot.tick;
            let kind = if self.cursor.is_some() {
                "HISTORY"
            } else if self.pending_until.is_some() {
                "STEPPING"
            } else if paused {
                "PAUSED"
            } else {
                "PAUSING"
            };
            rows.push(format!(
                "GT {tick}  {kind}  HISTORY {}/{}",
                self.history.len(),
                CAPACITY
            ));
            rows.push(format!(
                "BLOCK UPDATES {}   SIGNALS {}   PISTON {}",
                report.block_updates.len(),
                report.redstone_updates.len(),
                report.piston_events.len(),
            ));
            for update in &report.block_updates {
                if report.debug_interactions.iter().any(|interaction| {
                    interaction.position == update.position && interaction.result.is_ok()
                }) {
                    continue;
                }
                append_colored_block_outline(
                    &mut frame.terrain,
                    update.position,
                    eye,
                    UPDATE_COLOR,
                );
            }
            for interaction in &report.debug_interactions {
                let color = if interaction.result.is_ok() {
                    INTERACTION_COLOR
                } else {
                    REJECTED_COLOR
                };
                append_colored_block_outline(&mut frame.terrain, interaction.position, eye, color);
            }
            for update in &report.redstone_updates {
                append_signal_arrow(
                    &mut frame.terrain,
                    update.source,
                    update.position,
                    eye,
                    if update.power == 0 {
                        SIGNAL_OFF_COLOR
                    } else {
                        SIGNAL_COLOR
                    },
                );
            }
            let total = report.block_updates.len()
                + report.redstone_updates.len()
                + report.piston_events.len()
                + report.debug_interactions.len();
            if total != 0 {
                let pages = total.div_ceil(EVENT_ROWS);
                let page = self.page.min(pages - 1);
                rows.push(format!("EVENTS PAGE {}/{}", page + 1, pages));
                let state_name = |id: rustcraft_world::BlockStateId| {
                    let name = names.get(id.registry_index()).copied().unwrap_or("UNKNOWN");
                    name.split_once(':')
                        .map_or(name, |(_, name)| name)
                        .to_ascii_uppercase()
                };
                let signal_rows = report.redstone_updates.iter().map(|update| {
                    let p = update.position;
                    let s = update.source;
                    (
                        format!(
                            "SIGNAL {} {} {} > {} {} {}  POWER {}",
                            s.x, s.y, s.z, p.x, p.y, p.z, update.power
                        ),
                        if update.power == 0 {
                            SIGNAL_OFF_COLOR
                        } else {
                            SIGNAL_COLOR
                        },
                    )
                });
                let block_rows = report.block_updates.iter().map(|update| {
                    let p = update.position;
                    (
                        format!(
                            "{} {} {}  {} > {}",
                            p.x,
                            p.y,
                            p.z,
                            state_name(update.before),
                            state_name(update.after)
                        ),
                        UPDATE_COLOR,
                    )
                });
                let interaction_rows = report.debug_interactions.iter().map(|event| {
                    let p = event.position;
                    let action = match event.action {
                        rustcraft_authority::BlockAction::Place => "USE/PLACE",
                        rustcraft_authority::BlockAction::Break => "BREAK",
                        rustcraft_authority::BlockAction::Pick => "PICK",
                    };
                    (
                        format!(
                            "{} {action} {} {} {}",
                            if event.result.is_ok() {
                                "INPUT"
                            } else {
                                "REJECTED"
                            },
                            p.x,
                            p.y,
                            p.z
                        ),
                        if event.result.is_ok() {
                            INTERACTION_COLOR
                        } else {
                            REJECTED_COLOR
                        },
                    )
                });
                let piston_rows = report.piston_events.iter().map(|trace| {
                    use rustcraft_authority::java_signal::PistonPower;
                    let block = trace.moving_position.unwrap_or(trace.event.base);
                    let power = match trace.power {
                        Some(PistonPower::Direct(_)) => "DIRECT",
                        Some(PistonPower::Quasi(_)) => "QC",
                        Some(PistonPower::Unpowered) => "UNPOWERED",
                        Some(PistonPower::Unavailable) => "UNKNOWN",
                        None => "",
                    };
                    (
                        format!(
                            "PISTON {:?} {:?} {} {} {} {power}",
                            trace.event.action, trace.stage, block.x, block.y, block.z,
                        ),
                        PISTON_COLOR,
                    )
                });
                event_colors.extend(
                    interaction_rows
                        .chain(piston_rows)
                        .chain(signal_rows)
                        .chain(block_rows)
                        .skip(page * EVENT_ROWS)
                        .take(EVENT_ROWS),
                );
            }
        }
        let mut lines: Vec<_> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                vec![(row.as_str(), if index == 0 { HUD_ACCENT } else { HUD_TEXT })]
            })
            .collect();
        lines.insert(
            4,
            vec![
                ("Magenta: block update", UPDATE_COLOR),
                ("  Amber: signal", SIGNAL_COLOR),
                ("  Blue: signal lost", SIGNAL_OFF_COLOR),
            ],
        );
        lines.insert(
            5,
            vec![
                ("Mint: planned or committed input", INTERACTION_COLOR),
                ("  Red: rejected input", REJECTED_COLOR),
                ("  Lime: piston events", PISTON_COLOR),
            ],
        );
        lines.extend(
            event_colors
                .iter()
                .map(|(text, color)| vec![(text.as_str(), *color)]),
        );
        frame
            .hud
            .extend(colored_hud(size, &lines, HudCorner::TopLeft, true));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_256_saved_views_and_returns_to_the_live_edge_without_advancing() {
        let mut world = rustcraft_authority::Server::bootstrap(42).unwrap();
        let mut replay = TickReplay::default();
        for _ in 0..260 {
            replay.record(&world.step(Vec::new()));
        }
        assert_eq!(replay.history.len(), 256);
        replay.open(true);
        for _ in 0..260 {
            replay.previous();
        }
        assert_eq!(replay.snapshot(&world.snapshot()).tick, 5);
        assert!(!replay.at_live_tick());
        for _ in 0..260 {
            replay.next();
        }
        assert!(replay.at_live_tick());
        assert_eq!(world.snapshot().tick, 260);
    }
}
