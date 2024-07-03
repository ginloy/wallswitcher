use animation::{Animation, Fade, Static};
use anyhow::Result;
use cli::Cli;
use rand::distributions::Distribution;
use std::{
    io::Write,
    time::{Duration, Instant},
};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        wlr_layer::{Anchor, Layer, LayerShell, LayerShellHandler, LayerSurface},
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{
        wl_output, wl_shm,
        wl_surface::{self},
    },
    Connection, QueueHandle,
};

mod animation;
mod cli;
mod image_loader;

struct State {
    registry_state: RegistryState,
    output_state: OutputState,
    compositor_state: CompositorState,
    shm_state: Shm,
    layer_shell: LayerShell,
    layer: LayerSurface,

    dimensions: (u32, u32),
    pool: SlotPool,
    animation: Box<dyn Animation>,
    need_redraw: bool,
}

// struct ImageViewer {
//     image: image::RgbaImage,
//     width: u32,
//     height: u32,
//     buffer: Option<Buffer>,
//     first_configure: bool,
//     damaged: bool,
// }

fn main() -> Result<()> {
    let (images, interval) = Cli::parse_and_validate()?;
    let mut rng = rand::thread_rng();
    let rand = rand::distributions::Uniform::from(0..images.len());
    let mut generator = rand.sample_iter(&mut rng);
    let conn = Connection::connect_to_env().expect("Failed to get connection to wayland server");
    let (globals, mut queue) = registry_queue_init::<State>(&conn).unwrap();
    let qh = queue.handle();

    let mut image =
        image::open(images.get(generator.next().unwrap()).unwrap()).expect("Not an image");
    let compositor_state = CompositorState::bind(&globals, &qh).expect("Compositor not available");
    let output_state = OutputState::new(&globals, &qh);
    let shm = Shm::bind(&globals, &qh).expect("Failed to get shm");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("Layer shell not available");
    let surface = compositor_state.create_surface(&qh);
    let layer =
        layer_shell.create_layer_surface(&qh, surface, Layer::Background, Some("wallpaper"), None);
    layer.set_anchor(Anchor::all());
    layer.set_size(0, 0);
    layer.set_exclusive_zone(-1);
    layer.commit();
    if let Ok(region) = Region::new(&compositor_state) {
        layer
            .wl_surface()
            .set_input_region(Some(region.wl_region()));
        region.wl_region().destroy();
    };
    let mut state = State {
        registry_state: RegistryState::new(&globals),
        output_state,
        compositor_state,
        pool: SlotPool::new(256 * 144 * 4, &shm).expect("failed to create pool"),
        shm_state: shm,
        layer_shell,
        layer,
        animation: Box::new(Static::new(image.clone())),
        dimensions: (0, 0),
        need_redraw: false,
    };

    let mut last_change = Instant::now();
    let freq = 50;
    let cycle_time = Duration::from_secs(1) / freq;

    loop {
        let start = Instant::now();
        queue.flush().unwrap();
        queue.prepare_read().and_then(|g| g.read().ok());
        queue.dispatch_pending(&mut state).unwrap();
        if last_change.elapsed() > interval {
            let path = images.get(generator.next().unwrap()).unwrap();
            println!("{}", path.display());
            let new_img = image::open(path).unwrap();
            let animation = Fade::new(Duration::from_secs(5), 24, new_img.clone(), image);
            state.animation = Box::new(animation);
            image = new_img;
            last_change = Instant::now();
            state.need_redraw = true;
        }
        if state.need_redraw {
            state.draw(&qh);
        }
        let elapsed = start.elapsed();
        if elapsed < cycle_time {
            std::thread::sleep(cycle_time - elapsed);
        }
    }
}

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
        // Not needed for this example.
        println!("Received scale change");
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
        // Not needed for this example.
        println!("Received transform");
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

impl OutputHandler for State {
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

impl LayerShellHandler for State {
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &smithay_client_toolkit::shell::wlr_layer::LayerSurface,
        _configure: smithay_client_toolkit::shell::wlr_layer::LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.dimensions = _configure.new_size;
        println!("{:?}", self.dimensions);
        // _layer.set_size(self.dimensions.0, self.dimensions.1);
        self.need_redraw = true;
    }

    fn closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &smithay_client_toolkit::shell::wlr_layer::LayerSurface,
    ) {
    }
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl State {
    pub fn draw(&mut self, _qh: &QueueHandle<Self>) {
        match self.animation.next(self.dimensions) {
            None => {
                if self.animation.finished() {
                    self.need_redraw = false;
                }
            }
            Some(frame) => {
                let (width, height) = self.dimensions;
                let stride = width * 4;
                let pool = &mut self.pool;

                let (buffer, mut canvas) = pool
                    .create_buffer(
                        width as i32,
                        height as i32,
                        stride as i32,
                        wl_shm::Format::Xbgr8888,
                    )
                    .expect("create buffer");

                // Draw to the window:
                // println!("{}", self.image.display());
                canvas.write_all(frame.as_slice()).unwrap();
                // Damage the entire window
                self.layer
                    .wl_surface()
                    .damage_buffer(0, 0, width as i32, height as i32);
                buffer
                    .attach_to(self.layer.wl_surface())
                    .expect("Error attaching buffer");
                self.layer.commit();
            }
        }
    }
}

delegate_compositor!(State);
delegate_output!(State);
delegate_shm!(State);

delegate_registry!(State);
delegate_layer!(State);

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState];

    // registry_handlers!(OutputState);
}
