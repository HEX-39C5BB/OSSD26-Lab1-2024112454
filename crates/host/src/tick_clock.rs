use std::time::Duration;

use rustcraft_authority::{GAME_TICK_INTERVAL, GAME_TICKS_PER_SECOND};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TickCommand {
    Query,
    Rate(f64),
    Freeze,
    Unfreeze,
    Step(u32),
    StopStep,
    Sprint(u32),
    StopSprint,
}

#[derive(Clone, Copy, Debug)]
pub struct TickStatus {
    pub rate: f64,
    pub frozen: bool,
    pub stepping: u32,
    pub sprinting: u32,
    pub last_tick: Duration,
}

pub type TickReply = Result<TickStatus, &'static str>;

pub(crate) struct TickClock {
    status: TickStatus,
}

impl Default for TickClock {
    fn default() -> Self {
        Self {
            status: TickStatus {
                rate: f64::from(GAME_TICKS_PER_SECOND),
                frozen: false,
                stepping: 0,
                sprinting: 0,
                last_tick: Duration::ZERO,
            },
        }
    }
}

impl TickClock {
    pub fn command(&mut self, command: TickCommand) -> TickReply {
        match command {
            TickCommand::Query => {}
            TickCommand::Rate(rate) if (1.0..=10_000.0).contains(&rate) => self.status.rate = rate,
            TickCommand::Rate(_) => return Err("GT/S MUST BE 1 TO 10000"),
            TickCommand::Freeze => {
                self.status.frozen = true;
                self.status.stepping = 0;
            }
            TickCommand::Unfreeze => {
                self.status.frozen = false;
                self.status.stepping = 0;
            }
            TickCommand::Step(count)
                if self.status.frozen && self.status.sprinting == 0 && count > 0 =>
            {
                self.status.stepping = count;
            }
            TickCommand::Step(_) => {
                return Err("STEP REQUIRES A FROZEN WORLD AND A POSITIVE COUNT");
            }
            TickCommand::StopStep if self.status.stepping > 0 => self.status.stepping = 0,
            TickCommand::StopStep => return Err("NO TICK STEPS IN PROGRESS"),
            TickCommand::Sprint(count) if count > 0 => {
                self.status.sprinting = count;
                self.status.stepping = 0;
            }
            TickCommand::Sprint(_) => return Err("SPRINT REQUIRES A POSITIVE COUNT"),
            TickCommand::StopSprint if self.status.sprinting > 0 => self.status.sprinting = 0,
            TickCommand::StopSprint => return Err("NO TICK SPRINT IN PROGRESS"),
        }
        Ok(self.status)
    }

    pub fn interval(&self) -> Duration {
        if self.status.sprinting > 0 {
            return Duration::ZERO;
        }
        let interval = Duration::from_secs_f64(self.status.rate.recip());
        if self.status.frozen && self.status.stepping == 0 {
            interval.max(GAME_TICK_INTERVAL)
        } else {
            interval
        }
    }

    pub fn advance_world(&mut self) -> bool {
        if self.status.sprinting > 0 {
            self.status.sprinting -= 1;
        } else if self.status.stepping > 0 {
            self.status.stepping -= 1;
        } else if self.status.frozen {
            return false;
        }
        true
    }

    pub fn record_tick(&mut self, elapsed: Duration) {
        self.status.last_tick = elapsed;
    }
}
