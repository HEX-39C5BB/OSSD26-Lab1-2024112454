use std::f64::consts::TAU;
use std::time::Instant;

use rustcraft_authority::{Capabilities, Effect, Mutation, Request, Server, ViewDistance};
use rustcraft_host::benchmark::percentiles;
use rustcraft_scene::draw::Crosshair;
use rustcraft_vulkan::{BenchmarkGpu, benchmark_terrain};
use rustcraft_world::ChunkPos;

use super::{Error, Scene, open_pack, preview_blocks};

const SEED: u64 = 42;
const SIZE: [u32; 2] = [1920, 1080];
const WARMUP: usize = 30;

pub(crate) fn run(radius: i64, frames: usize, gpu: BenchmarkGpu) -> Result<(), Error> {
    if !(i64::from(ViewDistance::MIN_RADIUS)..=i64::from(ViewDistance::MAX_RADIUS))
        .contains(&radius)
    {
        return Err("terrain benchmark radius must be between 1 and 16".into());
    }
    if !(1..=10_000).contains(&frames) {
        return Err("terrain benchmark --frames must be between 1 and 10000".into());
    }

    let mut server = Server::playable(SEED)?;
    activate_radius(&mut server, radius)?;
    preview_blocks(&mut server)?;
    let snapshot = server.snapshot();
    let width_in_chunks = 2 * radius + 1;
    if snapshot.chunks.len() as i64 != width_in_chunks * width_in_chunks {
        return Err("terrain benchmark active chunk count does not match its radius".into());
    }
    let mut scene = Scene::new(&server, &open_pack(None)?)?;
    let start = Instant::now();
    scene.update_meshes(&snapshot);
    let mesh_ns = start.elapsed().as_nanos();
    let quads = scene
        .meshes
        .values()
        .map(|mesh| mesh.quads.len())
        .sum::<usize>();

    println!(
        "event=terrain_benchmark_scene scenario=flat_rotation_v1 seed={SEED} radius={radius} \
         width={} height={} warmup_frames={WARMUP} sampled_frames={frames} debug_assertions={}",
        SIZE[0],
        SIZE[1],
        cfg!(debug_assertions),
    );
    println!(
        "event=terrain_benchmark_mesh chunks={} quads={quads} build_ns={mesh_ns}",
        scene.meshes.len()
    );
    let samples = benchmark_terrain(SIZE, gpu, WARMUP, frames, |index| {
        let yaw = TAU * (index % frames) as f64 / frames as f64;
        scene.frame(
            &snapshot,
            SIZE,
            [0.5, 3.3, 0.2],
            yaw,
            -0.37,
            Crosshair::Idle,
        )
    })?;
    log_samples(
        "cpu_prepare",
        samples.iter().map(|sample| sample.prepare_ns),
    );
    log_samples("gpu_render", samples.iter().map(|sample| sample.gpu_ns));
    log_samples(
        "offscreen_frame",
        samples.iter().map(|sample| sample.frame_ns),
    );
    Ok(())
}

fn activate_radius(server: &mut Server, radius: i64) -> Result<(), Error> {
    let initial = server.snapshot();
    let session = server.open_local_session(Capabilities::P1_HOST);
    let requests = (-radius..=radius)
        .flat_map(|x| (-radius..=radius).map(move |z| ChunkPos { x, z }))
        .filter(|position| !initial.chunks.contains_key(position))
        .enumerate()
        .map(|(index, position)| Request {
            session,
            operation: index as u64 + 1,
            mutation: Mutation::SetChunkActive {
                position,
                active: true,
            },
        })
        .collect::<Vec<_>>();
    drop(initial);
    if !requests.is_empty() {
        let count = requests.len();
        let report = server.step(requests);
        if report.acks.len() != count
            || report
                .acks
                .iter()
                .any(|ack| !matches!(ack.result, Ok(Effect::ChunkChanged(_))))
        {
            return Err("terrain benchmark failed to activate its full radius".into());
        }
    }
    server.close_session(session);
    Ok(())
}

fn log_samples(metric: &str, values: impl Iterator<Item = u128>) {
    let mut samples: Vec<_> = values.collect();
    let [p50, p95, p99] = percentiles(&mut samples);
    println!(
        "event=terrain_benchmark_frame metric={metric} samples={} \
         p50_ns={p50} p95_ns={p95} p99_ns={p99}",
        samples.len()
    );
}
