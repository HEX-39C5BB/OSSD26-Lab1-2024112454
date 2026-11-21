use std::time::{Duration, Instant};

use ash::vk;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::context::Context;
use crate::frames::{Frame, Frames};

pub(crate) type Error = Box<dyn std::error::Error>;

pub(crate) fn vk_error(operation: &'static str) -> impl FnOnce(vk::Result) -> Error {
    move |error| format!("{operation}: {error}").into()
}

pub fn run(smoke: bool, poll_host: impl FnMut() -> Result<(), Error>) -> Result<(), Error> {
    let event_loop = EventLoop::new()?;
    let mut app = App {
        window: None,
        smoke,
        presented: false,
        error: None,
        poll_host,
    };
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error);
    }
    if smoke && !app.presented {
        return Err("window closed before presenting a Vulkan frame".into());
    }
    Ok(())
}

struct App<F> {
    window: Option<GraphicsWindow>,
    smoke: bool,
    presented: bool,
    error: Option<Error>,
    poll_host: F,
}

struct GraphicsWindow {
    frames: Option<Frames>,
    context: Context,
    window: Window,
}

impl GraphicsWindow {
    fn new(event_loop: &ActiveEventLoop) -> Result<Self, Error> {
        let window = event_loop.create_window(
            Window::default_attributes()
                .with_title("RustCraft")
                .with_inner_size(LogicalSize::new(800, 480)),
        )?;
        let context = Context::new(&window)?;
        Ok(Self {
            frames: None,
            context,
            window,
        })
    }

    fn draw(&mut self) -> Result<bool, Error> {
        if self.frames.is_none() {
            self.frames = Frames::new(&self.context, self.window.inner_size())?;
        }
        let Some(frames) = &self.frames else {
            return Ok(false);
        };
        match frames.clear_and_present(&self.context)? {
            Frame::Presented { suboptimal } => {
                if suboptimal {
                    self.recreate();
                }
                Ok(true)
            }
            Frame::Outdated => {
                self.recreate();
                Ok(false)
            }
        }
    }

    fn recreate(&mut self) {
        self.frames = None;
        self.window.request_redraw();
    }
}

impl<F: FnMut() -> Result<(), Error>> ApplicationHandler for App<F> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        match GraphicsWindow::new(event_loop) {
            Ok(window) => {
                window.window.request_redraw();
                self.window = Some(window);
            }
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(window) = &mut self.window else {
            return;
        };
        if id != window.window.id() {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => window.recreate(),
            WindowEvent::RedrawRequested => match window.draw() {
                Ok(true) if !self.presented => {
                    self.presented = true;
                    eprintln!("event=vulkan_first_frame_presented");
                    if self.smoke {
                        event_loop.exit();
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    self.error = Some(error);
                    event_loop.exit();
                }
            },
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = (self.poll_host)() {
            self.error = Some(error);
            event_loop.exit();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(50),
        ));
    }
}
