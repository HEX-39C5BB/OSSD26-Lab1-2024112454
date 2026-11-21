use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustcraft_scene::draw::Frame;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{
    DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use super::BenchmarkGpu;
use super::gpu::Gpu;
use super::swapchain::WindowRenderer;
use crate::window::Error;

const FRAME_TIME: Duration = Duration::from_millis(16);

#[derive(Clone, Copy)]
pub enum Interaction {
    Break,
    Place,
    Pick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKey {
    Toggle,
    Up,
    Down,
    Left,
    Right,
    Accept,
}

pub enum ConsoleKey {
    Open,
    Close,
    Submit,
    Previous,
    Next,
    Complete,
    CompleteBackward,
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Text(String),
}

#[derive(Clone, Copy, Default)]
pub struct FrameTiming {
    pub last_interval: Option<Duration>,
    pub average_interval: Option<Duration>,
}

#[derive(Default)]
pub struct Controls {
    pub forward: i8,
    pub strafe: i8,
    pub jump: bool,
    pub descend: bool,
    pub jump_presses: u8,
    pub mouse: [f64; 2],
    pub hotbar: Option<usize>,
    /// Positive values move toward the previous slot (wheel up).
    pub hotbar_scroll: i32,
    pub interaction: Option<Interaction>,
    pub break_held: bool,
    pub toggle_inventory: bool,
    pub cursor: Option<[f64; 2]>,
    pub ui_click: Option<([f64; 2], Interaction)>,
    pub console: Vec<ConsoleKey>,
    pub captured: bool,
    pub menu: Vec<MenuKey>,
    pub toggle_debug: bool,
    pub toggle_chunk_border: bool,
    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    pub toggle_tick_replay: bool,
    pub frame_timing: FrameTiming,
    pub graphics: Option<Arc<super::GraphicsInfo>>,
    pub surface_format: Option<Arc<str>>,
}

pub struct DesktopFrame {
    pub frame: Frame,
    pub paused: bool,
    pub console_open: bool,
}

pub struct PresentedFrame {
    pub size: [u32; 2],
    pub interval: Option<Duration>,
}

#[derive(Clone, Copy)]
enum WindowMode {
    Interactive,
    Smoke,
    Benchmark { size: [u32; 2], gpu: BenchmarkGpu },
}

impl WindowMode {
    fn is_benchmark(self) -> bool {
        matches!(self, Self::Benchmark { .. })
    }
}

pub fn run_terrain(
    smoke: bool,
    frame: impl FnMut(Controls, [u32; 2]) -> Result<DesktopFrame, Error>,
) -> Result<(), Error> {
    let mode = if smoke {
        WindowMode::Smoke
    } else {
        WindowMode::Interactive
    };
    run(mode, frame, |_| Ok(false))
}

pub fn run_terrain_window_benchmark(
    size: [u32; 2],
    gpu: BenchmarkGpu,
    frame: impl FnMut(Controls, [u32; 2]) -> Result<DesktopFrame, Error>,
    on_present: impl FnMut(PresentedFrame) -> Result<bool, Error>,
) -> Result<(), Error> {
    run(WindowMode::Benchmark { size, gpu }, frame, on_present)
}

fn run(
    mode: WindowMode,
    frame: impl FnMut(Controls, [u32; 2]) -> Result<DesktopFrame, Error>,
    on_present: impl FnMut(PresentedFrame) -> Result<bool, Error>,
) -> Result<(), Error> {
    let events = EventLoop::new()?;
    let now = Instant::now();
    let mut app = Game {
        frame,
        on_present,
        window: None,
        error: None,
        mode,
        presented: false,
        benchmark_complete: false,
        benchmark_deadline: mode.is_benchmark().then(|| now + Duration::from_secs(90)),
        next: now,
    };
    events.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error);
    }
    match mode {
        WindowMode::Smoke if !app.presented => {
            Err("window closed before presenting terrain".into())
        }
        WindowMode::Benchmark { .. } if !app.benchmark_complete => {
            Err("window closed before the streaming benchmark completed".into())
        }
        _ => Ok(()),
    }
}

struct Game<F, P> {
    frame: F,
    on_present: P,
    window: Option<GameWindow>,
    error: Option<Error>,
    mode: WindowMode,
    presented: bool,
    benchmark_complete: bool,
    benchmark_deadline: Option<Instant>,
    next: Instant,
}

struct GameWindow {
    renderer: WindowRenderer,
    window: Window,
    keys: BTreeSet<KeyCode>,
    controls: Controls,
    menu: Vec<MenuKey>,
    paused: bool,
    console_open: bool,
    console_shift: bool,
    resume_capture: ResumeCapture,
    scroll: HotbarScroll,
    debug_keys: DebugKeys,
    frame_timing: FrameTiming,
    last_present: Option<Instant>,
}

#[derive(Default)]
struct DebugKeys {
    f3_down: bool,
    chord_used: bool,
    toggle_panel: bool,
    toggle_border: bool,
    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    toggle_replay: bool,
}

impl DebugKeys {
    fn key(&mut self, code: KeyCode, pressed: bool, repeat: bool) -> bool {
        match code {
            KeyCode::F3 => {
                if pressed && !repeat && !self.f3_down {
                    self.f3_down = true;
                    self.chord_used = false;
                } else if !pressed && std::mem::take(&mut self.f3_down) && !self.chord_used {
                    self.toggle_panel ^= true;
                }
                true
            }
            KeyCode::KeyG if self.f3_down => {
                if pressed && !repeat {
                    self.chord_used = true;
                    self.toggle_border ^= true;
                }
                true
            }
            #[cfg(any(debug_assertions, feature = "dev-tools"))]
            KeyCode::KeyR if self.f3_down => {
                if pressed && !repeat {
                    self.chord_used = true;
                    self.toggle_replay ^= true;
                }
                true
            }
            _ => false,
        }
    }

    fn cancel_chord(&mut self) {
        self.f3_down = false;
    }
}

#[derive(Default)]
struct HotbarScroll {
    remainder: f64,
}

impl HotbarScroll {
    fn push(&mut self, delta: MouseScrollDelta) -> i32 {
        let amount = match delta {
            MouseScrollDelta::LineDelta(_, y) => f64::from(y),
            MouseScrollDelta::PixelDelta(position) => position.y / 40.0,
        };
        if !amount.is_finite() || amount == 0.0 {
            return 0;
        }
        if self.remainder.signum() != amount.signum() {
            self.remainder = 0.0;
        }
        let total = self.remainder + amount.clamp(-1024.0, 1024.0);
        let steps = total.trunc() as i32;
        self.remainder = total - f64::from(steps);
        steps
    }
}

#[derive(Default)]
struct ResumeCapture(bool);

impl ResumeCapture {
    fn remember(&mut self, captured: bool) {
        self.0 |= captured;
    }

    fn resume(&mut self, focused: bool) -> bool {
        std::mem::take(&mut self.0) && focused
    }
}

impl GameWindow {
    fn new(events: &ActiveEventLoop, mode: WindowMode) -> Result<Self, Error> {
        let attrs = Window::default_attributes().with_min_inner_size(LogicalSize::new(320, 200));
        let attrs = match mode {
            WindowMode::Benchmark {
                size: [width, height],
                ..
            } => attrs
                .with_title("RustCraft Streaming Benchmark")
                .with_inner_size(PhysicalSize::new(width, height))
                .with_resizable(false),
            _ => attrs
                .with_title("RustCraft")
                .with_inner_size(LogicalSize::new(1024, 640)),
        };
        let window = events.create_window(attrs)?;
        let gpu = match mode {
            WindowMode::Benchmark { gpu, .. } => gpu.select(Some(&window), false)?,
            _ => Gpu::new(Some(&window))?,
        };
        let renderer = WindowRenderer::new(gpu)?;
        Ok(Self {
            renderer,
            window,
            keys: BTreeSet::new(),
            controls: Controls::default(),
            menu: Vec::new(),
            paused: false,
            console_open: false,
            console_shift: false,
            resume_capture: ResumeCapture::default(),
            scroll: HotbarScroll::default(),
            debug_keys: DebugKeys::default(),
            frame_timing: FrameTiming::default(),
            last_present: None,
        })
    }

    fn capture(&mut self) -> Result<(), Error> {
        self.window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| self.window.set_cursor_grab(CursorGrabMode::Confined))?;
        self.window.set_cursor_visible(false);
        self.controls.captured = true;
        Ok(())
    }

    fn release(&mut self) {
        if self.controls.captured {
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
            self.window.set_cursor_visible(true);
        }
        self.keys.clear();
        self.debug_keys.cancel_chord();
        self.controls = Controls::default();
        self.scroll = HotbarScroll::default();
    }

    fn keyboard(&mut self, code: KeyCode, pressed: bool, repeat: bool, text: Option<&str>) {
        if self.console_open {
            if matches!(code, KeyCode::ShiftLeft | KeyCode::ShiftRight) {
                self.console_shift = pressed;
                return;
            }
            if pressed {
                let key = match code {
                    KeyCode::Escape => Some(ConsoleKey::Close),
                    KeyCode::Enter | KeyCode::NumpadEnter if !repeat => Some(ConsoleKey::Submit),
                    KeyCode::ArrowUp => Some(ConsoleKey::Previous),
                    KeyCode::ArrowDown => Some(ConsoleKey::Next),
                    KeyCode::Tab if self.console_shift => Some(ConsoleKey::CompleteBackward),
                    KeyCode::Tab => Some(ConsoleKey::Complete),
                    KeyCode::Backspace => Some(ConsoleKey::Backspace),
                    KeyCode::Delete => Some(ConsoleKey::Delete),
                    KeyCode::ArrowLeft => Some(ConsoleKey::Left),
                    KeyCode::ArrowRight => Some(ConsoleKey::Right),
                    KeyCode::Home => Some(ConsoleKey::Home),
                    KeyCode::End => Some(ConsoleKey::End),
                    _ => text
                        .filter(|value| value.bytes().all(|c| c == b' ' || c.is_ascii_graphic()))
                        .map(|value| ConsoleKey::Text(value.to_string())),
                };
                if let Some(key) = key {
                    self.controls.console.push(key);
                }
            }
            return;
        }
        if code == KeyCode::Slash && pressed && !repeat {
            self.controls.console.push(ConsoleKey::Open);
            return;
        }
        if self.debug_keys.key(code, pressed, repeat) {
            return;
        }
        if !pressed {
            self.keys.remove(&code);
            return;
        }
        if repeat {
            return;
        }
        if code == KeyCode::Escape {
            if !self.paused {
                self.resume_capture.remember(self.controls.captured);
            }
            self.release();
            self.menu.push(MenuKey::Toggle);
            return;
        }
        if code == KeyCode::KeyE {
            self.controls.toggle_inventory ^= true;
            return;
        }
        let menu = match code {
            KeyCode::ArrowUp => Some(MenuKey::Up),
            KeyCode::ArrowDown => Some(MenuKey::Down),
            KeyCode::ArrowLeft => Some(MenuKey::Left),
            KeyCode::ArrowRight => Some(MenuKey::Right),
            KeyCode::Enter | KeyCode::NumpadEnter => Some(MenuKey::Accept),
            _ => None,
        };
        if let Some(menu) = menu {
            self.menu.push(menu);
        }
        if !self.controls.captured {
            return;
        }
        if self.keys.insert(code) && code == KeyCode::Space {
            self.controls.jump_presses = (self.controls.jump_presses + 1).min(2);
        }
        const HOTBAR: [KeyCode; 9] = [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
        ];
        if let Some(slot) = HOTBAR.iter().position(|&key| key == code) {
            self.controls.hotbar = Some(slot);
            self.controls.hotbar_scroll = 0;
            self.scroll = HotbarScroll::default();
        }
    }

    fn input(&mut self) -> Controls {
        let mut state = std::mem::take(&mut self.controls);
        self.controls.captured = state.captured;
        self.controls.cursor = state.cursor;
        self.controls.break_held = state.break_held;
        let down = |key| i8::from(self.keys.contains(&key));
        state.forward = down(KeyCode::KeyW) - down(KeyCode::KeyS);
        state.strafe = down(KeyCode::KeyD) - down(KeyCode::KeyA);
        state.jump = self.keys.contains(&KeyCode::Space);
        state.descend =
            self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight);
        state.menu = std::mem::take(&mut self.menu);
        state.toggle_debug = std::mem::take(&mut self.debug_keys.toggle_panel);
        state.toggle_chunk_border = std::mem::take(&mut self.debug_keys.toggle_border);
        #[cfg(any(debug_assertions, feature = "dev-tools"))]
        {
            state.toggle_tick_replay = std::mem::take(&mut self.debug_keys.toggle_replay);
        }
        state.frame_timing = self.frame_timing;
        state.graphics = Some(self.renderer.graphics().clone());
        state.surface_format = self.renderer.surface_format().cloned();
        state
    }

    fn record_present(&mut self, now: Instant) {
        if let Some(previous) = self.last_present.replace(now) {
            let interval = now.duration_since(previous);
            self.frame_timing.last_interval = Some(interval);
            self.frame_timing.average_interval = Some(
                self.frame_timing
                    .average_interval
                    .map_or(interval, |average| {
                        average.mul_f64(0.85) + interval.mul_f64(0.15)
                    }),
            );
        }
    }

    fn set_paused(&mut self, paused: bool, focused: bool) {
        if paused && !self.paused {
            self.resume_capture.remember(self.controls.captured);
            self.release();
        }
        self.paused = paused;
        if !paused
            && self.resume_capture.resume(focused)
            && let Err(error) = self.capture()
        {
            eprintln!("event=cursor_capture_failed action=resume error={error:?}");
        }
    }
}

impl<F, P> ApplicationHandler for Game<F, P>
where
    F: FnMut(Controls, [u32; 2]) -> Result<DesktopFrame, Error>,
    P: FnMut(PresentedFrame) -> Result<bool, Error>,
{
    fn resumed(&mut self, events: &ActiveEventLoop) {
        if self.window.is_none() {
            match GameWindow::new(events, self.mode) {
                Ok(window) => {
                    window.window.request_redraw();
                    self.window = Some(window);
                }
                Err(error) => {
                    self.error = Some(error);
                    events.exit();
                }
            }
        }
    }

    fn window_event(&mut self, events: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(window) = &mut self.window else {
            return;
        };
        if window.window.id() != id {
            return;
        }
        match event {
            WindowEvent::CloseRequested => events.exit(),
            WindowEvent::Focused(false) if !self.mode.is_benchmark() => {
                window.release();
                window.last_present = None;
                window.frame_timing = FrameTiming::default();
                let size = window.window.inner_size();
                // An unfocused window might stop receiving redraw events.
                match (self.frame)(window.input(), [size.width.max(1), size.height.max(1)]) {
                    Ok(frame) => window.set_paused(frame.paused, false),
                    Err(error) => {
                        self.error = Some(error);
                        events.exit();
                    }
                }
            }
            WindowEvent::Resized(_) => {
                window.renderer.resize();
                window.window.request_redraw();
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } if !self.mode.is_benchmark() => {
                if let PhysicalKey::Code(code) = event.physical_key
                    && !(is_synthetic
                        && matches!(
                            code,
                            KeyCode::F3 | KeyCode::KeyG | KeyCode::KeyR | KeyCode::Space
                        ))
                {
                    window.keyboard(
                        code,
                        event.state == ElementState::Pressed,
                        event.repeat,
                        event.text.as_deref(),
                    );
                }
            }
            WindowEvent::MouseWheel { delta, .. }
                if !self.mode.is_benchmark()
                    && window.controls.captured
                    && !window.paused
                    && !window.console_open =>
            {
                let steps = window.scroll.push(delta);
                window.controls.hotbar_scroll = window.controls.hotbar_scroll.saturating_add(steps);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } if !self.mode.is_benchmark() => {
                if !window.controls.captured {
                    if window.paused {
                        let click = match button {
                            MouseButton::Left => Some(Interaction::Break),
                            MouseButton::Right => Some(Interaction::Place),
                            MouseButton::Middle => Some(Interaction::Pick),
                            _ => None,
                        };
                        window.controls.ui_click = window.controls.cursor.zip(click);
                    } else if !window.controls.toggle_inventory
                        && !window.menu.contains(&MenuKey::Toggle)
                        && button == MouseButton::Left
                        && let Err(error) = window.capture()
                    {
                        eprintln!("event=cursor_capture_failed error={error:?}");
                    }
                } else {
                    if button == MouseButton::Left {
                        window.controls.break_held = true;
                    }
                    window.controls.interaction = match button {
                        MouseButton::Left => Some(Interaction::Break),
                        MouseButton::Right => Some(Interaction::Place),
                        MouseButton::Middle => Some(Interaction::Pick),
                        _ => window.controls.interaction,
                    };
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } if !self.mode.is_benchmark() => {
                window.controls.break_held = false;
            }
            WindowEvent::CursorMoved { position, .. } if !self.mode.is_benchmark() => {
                window.controls.cursor = Some([position.x, position.y]);
            }
            WindowEvent::RedrawRequested => {
                let size = window.window.inner_size();
                let result = (self.frame)(window.input(), [size.width.max(1), size.height.max(1)])
                    .and_then(|frame| {
                        window.set_paused(frame.paused, window.window.has_focus());
                        window.console_open = frame.console_open;
                        window
                            .renderer
                            .draw([size.width, size.height], &frame.frame)
                    });
                match result {
                    Ok(Some(size)) => {
                        window.record_present(Instant::now());
                        if !self.presented {
                            self.presented = true;
                            eprintln!("event=vulkan_terrain_first_frame_presented");
                            if matches!(self.mode, WindowMode::Smoke) {
                                events.exit();
                            }
                        }
                        if let WindowMode::Benchmark { size: expected, .. } = self.mode {
                            let result = if size == expected {
                                (self.on_present)(PresentedFrame {
                                    size,
                                    interval: window.frame_timing.last_interval,
                                })
                            } else {
                                Err(format!(
                                    "streaming benchmark requires {}x{} pixels, got {}x{}",
                                    expected[0], expected[1], size[0], size[1]
                                )
                                .into())
                            };
                            match result {
                                Ok(true) => {
                                    self.benchmark_complete = true;
                                    events.exit();
                                }
                                Ok(false) => {}
                                Err(error) => {
                                    self.error = Some(error);
                                    events.exit();
                                }
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.error = Some(error);
                        events.exit();
                    }
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let (Some(window), DeviceEvent::MouseMotion { delta: (x, y) }) =
            (&mut self.window, event)
            && window.controls.captured
            && x.is_finite()
            && y.is_finite()
        {
            window.controls.mouse[0] += x;
            window.controls.mouse[1] += y;
        }
    }

    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        let now = Instant::now();
        if self
            .benchmark_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.error = Some("streaming benchmark timed out".into());
            events.exit();
            return;
        }
        if now >= self.next {
            if let Some(window) = &self.window {
                window.window.request_redraw();
            }
            self.next = now + FRAME_TIME;
        }
        events.set_control_flow(ControlFlow::WaitUntil(self.next));
    }
}

#[cfg(test)]
mod tests {
    use super::{DebugKeys, ResumeCapture};
    use winit::keyboard::KeyCode;

    #[test]
    fn toggles_debug_on_release_and_consumes_f3_when_used_as_a_chord() {
        let mut keys = DebugKeys::default();
        keys.key(KeyCode::F3, true, false);
        keys.key(KeyCode::F3, true, true);
        assert!(!keys.toggle_panel);
        keys.key(KeyCode::F3, false, false);
        assert!(std::mem::take(&mut keys.toggle_panel));

        assert!(!keys.key(KeyCode::KeyG, true, false));
        keys.key(KeyCode::F3, true, false);
        keys.key(KeyCode::KeyG, true, false);
        keys.key(KeyCode::KeyG, true, true);
        keys.key(KeyCode::KeyG, false, false);
        keys.key(KeyCode::F3, false, false);
        assert!(keys.toggle_border);
        assert!(!keys.toggle_panel);

        keys.key(KeyCode::F3, true, false);
        keys.cancel_chord();
        keys.key(KeyCode::F3, false, false);
        assert!(!keys.toggle_panel);

        #[cfg(any(debug_assertions, feature = "dev-tools"))]
        {
            keys.key(KeyCode::F3, true, false);
            assert!(keys.key(KeyCode::KeyR, true, false));
            keys.key(KeyCode::KeyR, true, true);
            keys.key(KeyCode::F3, false, false);
            assert!(keys.toggle_replay);
            assert!(!keys.toggle_panel);
        }
    }

    #[test]
    fn accumulates_fractional_wheel_motion_and_resets_on_direction_change() {
        use super::HotbarScroll;
        use winit::dpi::PhysicalPosition;
        use winit::event::MouseScrollDelta::{LineDelta, PixelDelta};
        let mut scroll = HotbarScroll::default();
        assert_eq!(scroll.push(LineDelta(0.0, 3.0)), 3);
        assert_eq!(scroll.push(LineDelta(0.0, -2.0)), -2);
        assert_eq!(scroll.push(LineDelta(0.0, 0.25)), 0);
        assert_eq!(scroll.push(LineDelta(0.0, 0.75)), 1);
        assert_eq!(
            scroll.push(PixelDelta(PhysicalPosition::new(50.0, 15.0))),
            0
        );
        assert_eq!(scroll.push(PixelDelta(PhysicalPosition::new(0.0, 25.0))), 1);
        assert_eq!(scroll.push(PixelDelta(PhysicalPosition::new(0.0, 30.0))), 0);
        assert_eq!(
            scroll.push(PixelDelta(PhysicalPosition::new(0.0, -40.0))),
            -1
        );
        assert_eq!(scroll.push(LineDelta(9.0, 0.0)), 0);
        assert_eq!(scroll.push(LineDelta(0.0, f32::NAN)), 0);
    }

    #[test]
    fn resumes_a_captured_game_once_and_respects_focus_loss() {
        let mut capture = ResumeCapture::default();
        capture.remember(false);
        assert!(!capture.resume(true));

        capture.remember(true);
        capture.remember(false);
        assert!(capture.resume(true));
        assert!(!capture.resume(true));

        capture.remember(true);
        assert!(!capture.resume(false));
        assert!(!capture.resume(true));
    }
}
