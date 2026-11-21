use std::time::Instant;

use rustcraft_authority::{
    Capabilities, Effect, Mutation, Request, Server, SessionId, ViewDistance,
};
use rustcraft_host::benchmark::percentiles;
use rustcraft_world::ChunkPos;

type Error = Box<dyn std::error::Error>;
const WARMUP_PAIRS: usize = 32;
const MAX_TICKS: u64 = 1_000_000;
const MAX_RADIUS: i64 = ViewDistance::MAX_RADIUS as i64;
const BOOT_RADIUS: i64 = 1;

pub(super) fn run(seed: u64, ticks: u64, radius: i64) -> Result<(), Error> {
    if !(2..=MAX_TICKS).contains(&ticks) || !ticks.is_multiple_of(2) {
        return Err(format!("benchmark --ticks must be even and between 2 and {MAX_TICKS}").into());
    }
    if !(i64::from(ViewDistance::MIN_RADIUS)..=MAX_RADIUS).contains(&radius) {
        return Err(format!("benchmark --radius must be between 1 and {MAX_RADIUS}").into());
    }

    let surrounding: Vec<_> = (-radius..=radius)
        .flat_map(|x| (-radius..=radius).map(move |z| ChunkPos { x, z }))
        .filter(|&position| position != ChunkPos::ORIGIN)
        .collect();
    let full_chunks = surrounding.len() + 1;

    let cold = Instant::now();
    let mut server = Server::playable(seed)?;
    let cold_boot_ns = cold.elapsed().as_nanos();
    let initial = server.snapshot();
    let boot_chunks = initial.chunks.len();
    let expected_boot_chunks = ((2 * BOOT_RADIUS + 1).pow(2)) as usize;
    if boot_chunks != expected_boot_chunks
        || initial
            .chunks
            .keys()
            .any(|position| position.x.abs() > BOOT_RADIUS || position.z.abs() > BOOT_RADIUS)
    {
        return Err("chunk_lifecycle_v1 requires a 3-by-3 startup area".into());
    }
    let expansion: Vec<_> = surrounding
        .iter()
        .copied()
        .filter(|position| !initial.chunks.contains_key(position))
        .collect();
    drop(initial);
    let session = server.open_local_session(Capabilities::P1_HOST);
    let mut operation = 0;
    let cold_expand_tick_ns = if expansion.is_empty() {
        0
    } else {
        switch_chunks(
            &mut server,
            session,
            &mut operation,
            &expansion,
            true,
            full_chunks,
        )?
    };
    let cold_setup_ns = cold.elapsed().as_nanos();
    println!(
        "event=benchmark_cold scenario=chunk_lifecycle_v1 seed={seed} radius={radius} \
         boot_chunks={boot_chunks} expansion_chunks={} active_chunks={full_chunks} \
         boot_ns={cold_boot_ns} expand_tick_ns={cold_expand_tick_ns} setup_ns={cold_setup_ns}",
        expansion.len()
    );

    let pairs = usize::try_from(ticks / 2)?;
    let mut deactivation = Vec::with_capacity(pairs);
    let mut activation = Vec::with_capacity(pairs);
    for pair in 0..(WARMUP_PAIRS + pairs) {
        let deactivate_ns =
            switch_chunks(&mut server, session, &mut operation, &surrounding, false, 1)?;
        let activate_ns = switch_chunks(
            &mut server,
            session,
            &mut operation,
            &surrounding,
            true,
            full_chunks,
        )?;
        if pair >= WARMUP_PAIRS {
            deactivation.push(deactivate_ns);
            activation.push(activate_ns);
        }
    }
    server.close_session(session);

    let mods = server
        .locked_mods()
        .iter()
        .map(|module| format!("{}@{}", module.id, module.version))
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "event=benchmark_summary scenario=chunk_lifecycle_v1 seed={seed} radius={radius} \
         warmup_ticks={} sampled_ticks={ticks} active_chunks_min=1 active_chunks_max={full_chunks} \
         peak_rss_kib={} debug_assertions={} mods={mods}",
        WARMUP_PAIRS * 2,
        peak_rss_kib().map_or_else(|| "unavailable".to_owned(), |value| value.to_string()),
        cfg!(debug_assertions),
    );
    log_ticks("deactivate", &mut deactivation);
    log_ticks("activate", &mut activation);
    Ok(())
}

fn switch_chunks(
    server: &mut Server,
    session: SessionId,
    operation: &mut u64,
    positions: &[ChunkPos],
    active: bool,
    expected_active: usize,
) -> Result<u128, Error> {
    let requests = positions
        .iter()
        .map(|&position| {
            *operation += 1;
            Request {
                session,
                operation: *operation,
                mutation: Mutation::SetChunkActive { position, active },
            }
        })
        .collect();
    let start = Instant::now();
    let report = server.step(requests);
    let elapsed = start.elapsed().as_nanos();
    let actual = report.snapshot.chunks.len();
    if actual != expected_active {
        return Err(
            format!("benchmark expected {expected_active} active chunks, found {actual}").into(),
        );
    }
    if report.acks.len() != positions.len() {
        return Err("benchmark is missing chunk acknowledgements".into());
    }
    for (ack, &position) in report.acks.iter().zip(positions) {
        if ack.result != Ok(Effect::ChunkChanged(position)) {
            return Err(format!("benchmark chunk {position:?} returned {:?}", ack.result).into());
        }
    }
    Ok(elapsed)
}

fn log_ticks(transition: &str, samples: &mut [u128]) {
    let [p50, p95, p99] = percentiles(samples);
    println!(
        "event=benchmark_tick_duration transition={transition} samples={} \
         p50_ns={p50} p95_ns={p95} p99_ns={p99}",
        samples.len(),
    );
}

fn peak_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let value = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))?;
    let mut fields = value.split_whitespace();
    let kib = fields.next()?.parse().ok()?;
    (fields.next() == Some("kB")).then_some(kib)
}
