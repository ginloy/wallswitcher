use animation::{Animation, Fade, Static};
use anyhow::Result;
use chrono::TimeDelta;
use cli::Cli;
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use std::{
    ffi::c_void,
    io::Write,
    ptr::NonNull,
    sync::mpsc,
    time::{Duration, Instant},
};
use timer::Timer;

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
        wl_output::{self, WlOutput},
        wl_shm, wl_surface,
    },
    Connection, Proxy, QueueHandle,
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
    let (mut imgloader, interval) = Cli::parse_and_validate()?;
    let conn = Connection::connect_to_env().expect("Failed to get connection to wayland server");
    let (globals, mut queue) = registry_queue_init::<State>(&conn).unwrap();
    let qh = queue.handle();

    let compositor_state = CompositorState::bind(&globals, &qh).expect("Compositor not available");
    let output_state = OutputState::new(&globals, &qh);
    let shm = Shm::bind(&globals, &qh).expect("Failed to get shm");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("Layer shell not available");
    let surface = compositor_state.create_surface(&qh);

    let output = output_state.outputs().find(WlOutput::is_alive).unwrap();
    let layer = layer_shell.create_layer_surface(
        &qh,
        surface,
        Layer::Background,
        Some("wallpaper"),
        Some(&output),
    );
    layer.set_anchor(Anchor::all());
    layer.set_size(0, 0);
    layer.set_exclusive_zone(-1);
    layer.commit();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let raw_layer_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
        NonNull::new(layer.wl_surface().id().as_ptr() as *mut c_void).unwrap(),
    ));
    let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
        NonNull::new(conn.backend().display_ptr() as *mut c_void).unwrap(),
    ));

    let surface = unsafe {
        instance
            .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_window_handle: raw_layer_handle,
                raw_display_handle,
            })
            .expect("Failed to create gpu surface")
    };

    println!("Surface: {surface:#?}");

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: Some(&surface),
        ..Default::default()
    }))
    .expect("Failed to get adapter");
    println!("{:#?}", adapter.get_info());

    let (device, gpu_queue) = pollster::block_on(adapter.request_device(&Default::default(), None))
        .expect("Failed to request device");
    println!("Device: {device:#?}");
    return Ok(());

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
        animation: Box::new(imgloader.load_static()?),
        dimensions: (0, 0),
        need_redraw: false,
    };

    let freq = 30;
    let cycle_time = Duration::from_secs(1) / freq;
    let timer = Timer::new();
    let (sender, receiver) = mpsc::channel();
    let interval = TimeDelta::from_std(interval)?;
    let _guard = timer.schedule_repeating(interval, move || {
        sender.send(imgloader.load_fade()).expect("Channel closed");
    });

    loop {
        let start = Instant::now();
        queue.flush().unwrap();
        queue.prepare_read().and_then(|g| g.read().ok());
        queue.dispatch_pending(&mut state).unwrap();
        if let Ok(anim) = receiver.try_recv() {
            match anim {
                Err(e) => {
                    eprintln!("Could not get animation: {e}");
                }
                Ok(a) => {
                    state.animation = Box::new(a);
                    state.need_redraw = true;
                }
            }
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
