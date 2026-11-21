use std::time::Duration;

pub const GAME_TICKS_PER_SECOND: u32 = 20;
pub const GAME_TICK_INTERVAL: Duration =
    Duration::from_nanos(1_000_000_000 / GAME_TICKS_PER_SECOND as u64);
pub const JAVA_REDSTONE_TICK_GAME_TICKS: u8 = 2;

/// Convert a delay expressed in Java 1.20.1 redstone ticks to the world clock.
pub(crate) fn java_redstone_delay(redstone_ticks: u8) -> u64 {
    u64::from(redstone_ticks) * u64::from(JAVA_REDSTONE_TICK_GAME_TICKS)
}
