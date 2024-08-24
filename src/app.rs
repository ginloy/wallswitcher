use image::DynamicImage;
use once_cell::sync::Lazy;
use rand::seq::SliceRandom;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};
use log::*;

use calloop::EventLoop;
use client::{
    globals::registry_queue_init,
    protocol::{
        wl_output::{self},
        wl_surface,
    },
    Connection, QueueHandle,
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop::{
            self,
            timer::{TimeoutAction, Timer},
        },
        calloop_wayland_source::WaylandSource,
        client,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        wlr_layer::{Anchor, Layer, LayerShell, LayerShellHandler, LayerSurface},
        WaylandSurface,
    },
};

const FPS: f32 = 60.0;
static FRAMETIME: Lazy<Duration> = Lazy::new(|| Duration::from_secs_f32(1.0 / FPS));
const MIN_FPS: f32 = 5.0;
static MAX_FRAMETIME: Lazy<Duration> = Lazy::new(|| Duration::from_secs_f32(1.0 / MIN_FPS));

use crate::{
    cli,
    render::{self, Animation},
};

#[derive(Clone, Copy, Debug, PartialEq)]
enum AppEvent {
    Draw,
}

pub struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    compositor_state: CompositorState,
    layer_shell: LayerShell,

    animation: Box<dyn Animation>,

    // drop ctx after animation
    ctx: render::Context,
    // Connection and Layer needs to be dropped after ctx
    conn: Connection,
    layer: LayerSurface,

    img_dir: PathBuf,
    interval: Duration,
    configured: bool,
    frame_timer: FrameTimer,
}

impl App {
    pub fn run() -> Result<()> {
        let (img_dir, interval) = cli::Cli::parse_and_validate()?;

        let conn =
            Connection::connect_to_env().context("Failed to get connection to wayland server")?;
        let (globals, queue) = registry_queue_init::<App>(&conn)?;
        let qh = queue.handle();

        let registry_state = RegistryState::new(&globals);
        let compositor_state =
            CompositorState::bind(&globals, &qh).expect("Compositor not available");
        let output_state = OutputState::new(&globals, &qh);
        let layer_shell = LayerShell::bind(&globals, &qh).expect("Layer shell not available");
        let surface = compositor_state.create_surface(&qh);

        let layer = layer_shell.create_layer_surface(
            &qh,
            surface,
            Layer::Background,
            Some("wallpaper"),
            None,
        );
        layer.set_anchor(Anchor::all());
        layer.set_size(0, 0);
        layer.set_exclusive_zone(-1);

        match Region::new(&compositor_state) {
            Ok(region) => {
                layer.set_input_region(Some(region.wl_region()));
                region.wl_region().destroy();
            }
            Err(e) => {
                warn!("Failed to set input region, background may not have cursor: {e}");
            }
        }

        layer.commit();

        let ctx = pollster::block_on(render::Context::new(&conn, &layer, (256, 256)));
        let animation = Box::new(crate::render::animation::Fade::new(
            &Self::load_random_img(&img_dir).unwrap(),
            &Self::load_random_img(&img_dir).unwrap(),
            Duration::from_secs(8),
            &ctx,
        ));

        let mut event_loop: EventLoop<App> = EventLoop::try_new()?;
        let event_loop_handler = event_loop.handle();

        let mut app = Self {
            conn,
            registry_state,
            output_state,
            compositor_state,
            layer_shell,
            ctx,
            animation,
            layer,

            img_dir,
            interval,
            configured: false,
            frame_timer: FrameTimer::new(FPS),
        };

        let _ = event_loop_handler.insert_source(
            Timer::from_deadline(app.frame_timer.next_frame()),
            |_, _, app| {
                if app.frame_timer.start() {
                    app.draw();
                    if app.animation.is_finished() {
                        app.frame_timer.set_fps(MIN_FPS);
                    } else {
                        app.frame_timer.set_fps(FPS);
                    }
                }
                TimeoutAction::ToInstant(app.frame_timer.next_frame())
            },
        );

        event_loop_handler
            .insert_source(Timer::from_duration(app.interval), |_, _, app| {
                match app.load_img() {
                    Ok(img) => {
                        app.animation.update_img(&img, &app.ctx);
                    }
                    Err(e) => {
                        error!("Could not load new img: {e}");
                    }
                }
                TimeoutAction::ToDuration(app.interval)
            })
            .map_err(|e| anyhow!("{e}"))?;

        let loop_signal = event_loop.get_signal();
        ctrlc::set_handler(move || {
            info!("SIGTERM/SIGINT/SIGHUP received, exiting");
            loop_signal.stop();
            loop_signal.wakeup();
        })
        .context("Failed to set SIG handlers")?;
        WaylandSource::new(app.conn.clone(), queue)
            .insert(event_loop.handle())
            .map_err(|e| anyhow!("{e}"))
            .context("Failed to insert wayland source into event loop")?;
        event_loop.run(None, &mut app, |_| ())?;
        Ok(())
    }

    fn draw(&mut self) {
        self.animation.render(&self.ctx);
    }

    fn load_random_img(dir: &Path) -> Result<DynamicImage> {
        let mut rng = rand::thread_rng();
        let mut files: Vec<_> = dir
            .read_dir()?
            .filter_map(Result::ok)
            .map(|d| d.path())
            .filter(|p| p.is_file())
            .collect();
        files.shuffle(&mut rng);
        files
            .into_iter()
            .filter_map(|p| {
                info!("Attempting to load {}", p.display());
                image::open(&p).ok()
            })
            .next()
            .map(|i| {
                info!("success");
                i
            })
            .with_context(|| format!("Unable to open any file from {} as an image", dir.display()))
    }

    fn load_img(&self) -> Result<DynamicImage> {
        Self::load_random_img(&self.img_dir)
    }
}
impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
        // Not needed for this example.
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
        // Not needed for this example.
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
        // Not needed for this example.
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
        // Not needed for this example.
    }
}

delegate_compositor!(App);
impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}
delegate_output!(App);

impl LayerShellHandler for App {
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &smithay_client_toolkit::shell::wlr_layer::LayerSurface,
        _configure: smithay_client_toolkit::shell::wlr_layer::LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.ctx.resize(_configure.new_size);
        self.configured = true;
        self.draw();
        // _layer.set_size(self.dimensions.0, self.dimensions.1);
    }

    fn closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &smithay_client_toolkit::shell::wlr_layer::LayerSurface,
    ) {
        warn!("Surface closed");
    }
}
delegate_layer!(App);

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState];
}
delegate_registry!(App);

struct FrameTimer {
    fps: f32,
    start: Instant,
}

impl FrameTimer {
    fn new(fps: f32) -> Self {
        Self {
            fps,
            start: Instant::now(),
        }
    }

    fn frametime(&self) -> Duration {
        Duration::from_secs_f32(1.0 / self.fps)
    }

    fn start(&mut self) -> bool {
        if (self.start + self.frametime()) > Instant::now() {
            return false;
        }
        self.start = Instant::now();
        true
    }

    fn next_frame(&self) -> Instant {
        self.start + self.frametime()
    }

    fn set_fps(&mut self, fps: f32) {
        self.fps = fps;
    }
}
