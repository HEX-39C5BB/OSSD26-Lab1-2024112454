#[cfg(target_os = "linux")]
mod context;
#[cfg(target_os = "linux")]
mod frames;
#[cfg(target_os = "linux")]
mod terrain;
#[cfg(target_os = "linux")]
mod window;

#[cfg(target_os = "linux")]
pub use context::probe;
#[cfg(target_os = "linux")]
pub use terrain::{
    BenchmarkGpu, ConsoleKey, Controls, DesktopFrame, FrameTiming, GraphicsInfo, Interaction,
    MenuKey, PresentedFrame, TerrainFrameSample, benchmark_terrain, render_terrain, run_terrain,
    run_terrain_window_benchmark,
};
#[cfg(target_os = "linux")]
pub use window::run;
