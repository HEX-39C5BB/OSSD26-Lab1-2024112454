use std::cell::Cell;
use std::collections::BTreeSet;
use std::f64::consts::PI;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use rustcraft_authority::{Mutation, PlayerInput, PlayerSnapshot, Server, Snapshot, ViewDistance};
use rustcraft_host::{LocalHost, benchmark::percentiles};
use rustcraft_scene::draw::Crosshair;
use rustcraft_vulkan::{BenchmarkGpu, DesktopFrame, run_terrain_window_benchmark};
use rustcraft_world::ChunkPos;

use super::{Error, Scene, open_pack};

const SIZE: [u32; 2] = [1920, 1080];
const SEED: u64 = 42;
const WARMUP_FRAMES: u32 = 30;
const CROSSINGS_PER_LEG: usize = 3;
const TURN_Z: f64 = 50.0;
const FINISH_Z: f64 = 0.75;
const VIEW_PITCH: f64 = -0.37;

pub(crate) fn run(distance: ViewDistance, gpu: BenchmarkGpu) -> Result<(), Error> {
    let mut server = Server::playable(SEED)?;
    server.set_local_view_distance(distance);
    let mut scene = Scene::new(&server, &open_pack(None)?)?;
    let host = LocalHost::start_world(server, None)?;
    let mut snapshot = host.initial.clone();
    let mut activations = ChunkActivations::new(&snapshot);
    let expected_chunks = (2 * usize::from(distance.radius()) + 1).pow(2);
    let frame_sample = Cell::new(None);
    let mut route = Route::Warmup { stable_frames: 0 };
    println!(
        "event=streaming_benchmark_scene scenario=flat_round_trip_v1 seed={SEED} radius={} \
         expected_chunks={expected_chunks} width={} height={} warmup_frames={WARMUP_FRAMES} \
         debug_assertions={}",
        distance.radius(),
        SIZE[0],
        SIZE[1],
        cfg!(debug_assertions),
    );

    let result = run_terrain_window_benchmark(
        SIZE,
        gpu,
        |_, size| {
            loop {
                match host.updates.try_recv() {
                    Ok(report) => {
                        if let Some(ack) = report.acks.iter().find(|ack| ack.result.is_err()) {
                            return Err(
                                format!("streaming movement rejected: {:?}", ack.result).into()
                            );
                        }
                        activations.observe(&snapshot, &report.snapshot);
                        snapshot = report.snapshot;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        return Err("streaming authority stopped".into());
                    }
                }
            }
            let player = snapshot
                .player
                .ok_or("missing streaming benchmark player")?;
            let start = Instant::now();
            let pending = scene.update_meshes_budgeted(&snapshot, player.block_position().chunk());
            let mesh_ns = start.elapsed().as_nanos();
            frame_sample.set(Some(Reading::new(
                &snapshot,
                player,
                expected_chunks,
                snapshot.chunks.len() - pending,
                mesh_ns,
                activations.since_crossing,
            )));
            let frame = scene.draw_frame(
                &snapshot,
                size,
                player.eye_position(),
                player.yaw,
                VIEW_PITCH,
                Crosshair::Idle,
            );
            Ok(DesktopFrame {
                frame,
                paused: false,
                console_open: false,
            })
        },
        |presented| {
            if presented.interval.is_none() {
                let [width, height] = presented.size;
                println!("event=streaming_benchmark_surface width={width} height={height}");
            }
            let reading = frame_sample
                .take()
                .ok_or("presented frame has no streaming reading")?;
            route.observe(reading, presented.interval, |operation, input| {
                host.submit(operation, Mutation::ControlPlayer(input))?;
                Ok(())
            })
        },
    );
    let stopped = host.shutdown();
    result.and(stopped)?;
    println!(
        "event=streaming_benchmark_complete radius={}",
        distance.radius()
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ActivationCounts {
    generated: usize,
    reactivated: usize,
}

struct ChunkActivations {
    seen: BTreeSet<ChunkPos>,
    center: Option<ChunkPos>,
    since_crossing: ActivationCounts,
}

impl ChunkActivations {
    fn new(initial: &Snapshot) -> Self {
        Self {
            seen: initial.chunks.keys().copied().collect(),
            center: initial.player.map(|player| player.block_position().chunk()),
            since_crossing: ActivationCounts::default(),
        }
    }

    fn observe(&mut self, previous: &Snapshot, next: &Snapshot) {
        let center = next.player.map(|player| player.block_position().chunk());
        if center != self.center {
            self.center = center;
            self.since_crossing = ActivationCounts::default();
        }
        // This scenario starts with a fresh world and retains every generated chunk in memory.
        for position in next
            .chunks
            .keys()
            .filter(|pos| !previous.chunks.contains_key(pos))
        {
            if self.seen.insert(*position) {
                self.since_crossing.generated += 1;
            } else {
                self.since_crossing.reactivated += 1;
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Reading {
    tick: u64,
    center: ChunkPos,
    z: f64,
    grounded: bool,
    pending_chunks: usize,
    pending_meshes: usize,
    mesh_ns: u128,
    activations: ActivationCounts,
}

impl Reading {
    fn new(
        snapshot: &Snapshot,
        player: PlayerSnapshot,
        expected_chunks: usize,
        meshes: usize,
        mesh_ns: u128,
        activations: ActivationCounts,
    ) -> Self {
        Self {
            tick: snapshot.tick,
            center: player.block_position().chunk(),
            z: player.position[2],
            grounded: player.grounded,
            pending_chunks: expected_chunks.saturating_sub(snapshot.chunks.len()),
            pending_meshes: snapshot.chunks.len().saturating_sub(meshes),
            mesh_ns,
            activations,
        }
    }

    fn settled(self) -> bool {
        self.grounded && self.pending_chunks == 0 && self.pending_meshes == 0
    }
}

enum Route {
    Warmup { stable_frames: u32 },
    Outward(LegSamples),
    Return(LegSamples),
}

impl Route {
    fn observe(
        &mut self,
        reading: Reading,
        interval: Option<Duration>,
        mut submit: impl FnMut(u64, PlayerInput) -> Result<(), Error>,
    ) -> Result<bool, Error> {
        match self {
            Self::Warmup { stable_frames } => {
                if reading.center != ChunkPos::ORIGIN {
                    return Err("streaming benchmark must start in the origin chunk".into());
                }
                if !reading.settled() || interval.is_none() {
                    *stable_frames = 0;
                    return Ok(false);
                }
                *stable_frames += 1;
                if *stable_frames == WARMUP_FRAMES {
                    submit(1, walk(0.0))?;
                    *self = Self::Outward(LegSamples::new(reading));
                }
            }
            Self::Outward(samples) => {
                samples.record(reading, interval, 1)?;
                if reading.z >= TURN_Z && reading.settled() {
                    samples.report("outward", reading)?;
                    submit(2, walk(PI))?;
                    *self = Self::Return(LegSamples::new(reading));
                }
            }
            Self::Return(samples) => {
                samples.record(reading, interval, -1)?;
                if reading.z <= FINISH_Z && reading.settled() {
                    samples.report("return", reading)?;
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

fn walk(yaw: f64) -> PlayerInput {
    PlayerInput {
        forward: 1,
        yaw,
        pitch: VIEW_PITCH,
        ..PlayerInput::default()
    }
}

struct LegSamples {
    first_tick: u64,
    previous: ChunkPos,
    crossings: Vec<Crossing>,
    peak_pending_chunks: usize,
    peak_pending_meshes: usize,
    intervals: Vec<u128>,
    mesh_updates: Vec<u128>,
}

impl LegSamples {
    fn new(reading: Reading) -> Self {
        Self {
            first_tick: reading.tick,
            previous: reading.center,
            crossings: Vec::with_capacity(CROSSINGS_PER_LEG),
            peak_pending_chunks: 0,
            peak_pending_meshes: 0,
            intervals: Vec::new(),
            mesh_updates: Vec::new(),
        }
    }

    fn record(
        &mut self,
        reading: Reading,
        interval: Option<Duration>,
        direction: i64,
    ) -> Result<(), Error> {
        let interval = interval
            .ok_or("missing interval between presented benchmark frames")?
            .as_nanos();
        if reading.center != self.previous {
            if reading.center.x != self.previous.x
                || reading.center.z != self.previous.z + direction
            {
                return Err(
                    "streaming benchmark left its route or skipped a chunk observation".into(),
                );
            }
            if self.crossings.len() == CROSSINGS_PER_LEG {
                return Err(
                    "streaming benchmark exceeded its route before the view settled".into(),
                );
            }
            self.crossings.push(Crossing::new(reading));
            self.previous = reading.center;
        }
        if let Some(crossing) = self
            .crossings
            .last_mut()
            .filter(|crossing| !crossing.settled)
        {
            crossing.record(reading, interval);
        }
        self.intervals.push(interval);
        self.mesh_updates.push(reading.mesh_ns);
        self.peak_pending_chunks = self.peak_pending_chunks.max(reading.pending_chunks);
        self.peak_pending_meshes = self.peak_pending_meshes.max(reading.pending_meshes);
        Ok(())
    }

    fn report(&mut self, leg: &str, ending: Reading) -> Result<(), Error> {
        if self.crossings.len() != CROSSINGS_PER_LEG || self.intervals.is_empty() {
            return Err("streaming benchmark did not complete its expected chunk crossings".into());
        }
        println!(
            "event=streaming_benchmark_leg leg={leg} crossed_chunks={} first_tick={} last_tick={} \
             peak_pending_chunks={} peak_pending_meshes={}",
            self.crossings.len(),
            self.first_tick,
            ending.tick,
            self.peak_pending_chunks,
            self.peak_pending_meshes,
        );
        log_samples(leg, "present_interval", &mut self.intervals);
        log_samples(leg, "mesh_update_cpu", &mut self.mesh_updates);
        for crossing in &self.crossings {
            crossing.report(leg);
        }
        Ok(())
    }
}

struct Crossing {
    center: ChunkPos,
    first_tick: u64,
    last_tick: u64,
    elapsed_ns: u128,
    samples: usize,
    max_interval_ns: u128,
    max_mesh_ns: u128,
    activations: ActivationCounts,
    settled: bool,
}

impl Crossing {
    fn new(reading: Reading) -> Self {
        Self {
            center: reading.center,
            first_tick: reading.tick,
            last_tick: reading.tick,
            elapsed_ns: 0,
            samples: 0,
            max_interval_ns: 0,
            max_mesh_ns: 0,
            activations: ActivationCounts::default(),
            settled: false,
        }
    }

    fn record(&mut self, reading: Reading, interval_ns: u128) {
        // Include the first frame's interval in the peak, but start the elapsed time at its present.
        if self.samples > 0 {
            self.elapsed_ns += interval_ns;
        }
        self.samples += 1;
        self.last_tick = reading.tick;
        self.max_interval_ns = self.max_interval_ns.max(interval_ns);
        self.max_mesh_ns = self.max_mesh_ns.max(reading.mesh_ns);
        self.activations = reading.activations;
        self.settled = reading.settled();
    }

    fn report(&self, leg: &str) {
        println!(
            "event=streaming_benchmark_crossing leg={leg} chunk_x={} chunk_z={} first_tick={} \
             last_tick={} settled={} elapsed_ns={} samples={} max_present_interval_ns={} \
             max_mesh_update_cpu_ns={} generated_chunks={} reactivated_chunks={}",
            self.center.x,
            self.center.z,
            self.first_tick,
            self.last_tick,
            self.settled,
            self.elapsed_ns,
            self.samples,
            self.max_interval_ns,
            self.max_mesh_ns,
            self.activations.generated,
            self.activations.reactivated,
        );
    }
}

fn log_samples(leg: &str, metric: &str, samples: &mut [u128]) {
    let [p50, p95, p99] = percentiles(samples);
    let max = samples[samples.len() - 1];
    println!(
        "event=streaming_benchmark_frame leg={leg} metric={metric} samples={} \
         p50_ns={p50} p95_ns={p95} p99_ns={p99} max_ns={max}",
        samples.len(),
    );
}

#[cfg(test)]
mod tests {
    use rustcraft_authority::Request;

    use super::*;

    #[test]
    fn crossing_measures_from_first_present_until_chunks_and_meshes_settle() {
        let origin = Reading {
            tick: 30,
            center: ChunkPos::ORIGIN,
            z: 15.9,
            grounded: true,
            pending_chunks: 0,
            pending_meshes: 0,
            mesh_ns: 0,
            activations: ActivationCounts::default(),
        };
        let mut samples = LegSamples::new(origin);
        let mut reading = Reading {
            tick: 31,
            center: ChunkPos { x: 0, z: 1 },
            z: 16.1,
            pending_chunks: 1,
            pending_meshes: 2,
            mesh_ns: 5,
            ..origin
        };
        samples
            .record(reading, Some(Duration::from_nanos(22)), 1)
            .unwrap();
        reading.tick += 1;
        reading.pending_chunks = 0;
        reading.mesh_ns = 6;
        samples
            .record(reading, Some(Duration::from_nanos(16)), 1)
            .unwrap();
        assert!(!samples.crossings[0].settled);
        reading.pending_meshes = 0;
        samples
            .record(reading, Some(Duration::from_nanos(17)), 1)
            .unwrap();
        samples
            .record(reading, Some(Duration::from_nanos(30)), 1)
            .unwrap();

        let crossing = &samples.crossings[0];
        assert!(crossing.settled);
        assert_eq!((crossing.first_tick, crossing.last_tick), (31, 32));
        assert_eq!((crossing.elapsed_ns, crossing.samples), (33, 3));
        assert_eq!(crossing.max_interval_ns, 22);
        assert_eq!(crossing.max_mesh_ns, 6);
        assert_eq!(samples.intervals.len(), 4);
    }

    #[test]
    fn round_trip_crosses_three_chunks_per_leg_at_each_view_distance() {
        for radius in [1, 4, 8, 12, 16] {
            let distance = ViewDistance::from_radius(radius).unwrap();
            let mut server = Server::playable(SEED).unwrap();
            server.set_local_view_distance(distance);
            let session = server.open_player_session().unwrap();
            let row_width = 2 * usize::from(distance.radius()) + 1;
            let expected_chunks = row_width.pow(2);
            let mut snapshot = server.snapshot();
            let mut activations = ChunkActivations::new(&snapshot);
            let mut route = Route::Warmup { stable_frames: 0 };
            let mut requests = Vec::new();
            let mut completed = false;
            for _ in 0..1200 {
                let report = server.step(std::mem::take(&mut requests));
                assert!(report.acks.iter().all(|ack| ack.result.is_ok()));
                activations.observe(&snapshot, &report.snapshot);
                snapshot = report.snapshot;
                let reading = Reading::new(
                    &snapshot,
                    snapshot.player.unwrap(),
                    expected_chunks,
                    snapshot.chunks.len(),
                    0,
                    activations.since_crossing,
                );
                completed = route
                    .observe(
                        reading,
                        Some(Duration::from_millis(16)),
                        |operation, input| {
                            requests.push(Request {
                                session,
                                operation,
                                mutation: Mutation::ControlPlayer(input),
                            });
                            Ok(())
                        },
                    )
                    .unwrap();
                if let Route::Outward(samples) | Route::Return(samples) = &route {
                    let expected = if matches!(route, Route::Outward(_)) {
                        ActivationCounts {
                            generated: row_width,
                            reactivated: 0,
                        }
                    } else {
                        ActivationCounts {
                            generated: 0,
                            reactivated: row_width,
                        }
                    };
                    for crossing in samples.crossings.iter().filter(|crossing| crossing.settled) {
                        assert_eq!(crossing.activations, expected);
                    }
                }
                if completed {
                    assert_eq!(reading.center, ChunkPos::ORIGIN);
                    assert!(reading.settled());
                    break;
                }
            }
            assert!(
                completed,
                "unfinished round trip at radius {}",
                distance.radius()
            );
        }
    }
}
