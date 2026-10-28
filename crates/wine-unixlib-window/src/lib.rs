//! `wine_unixlib_window.dll` (i686 PE, marked builtin) and its unixlib (the
//! same source built natively): a window drawn by winit and wgpu, the
//! renderer's stack, on a native thread of its own, presenting frames
//! continuously while 32-bit callers block in calls waiting for them. The
//! caller is `wine-unixlib-window-app`.

pub mod args;

#[cfg(not(target_arch = "x86"))]
mod native {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Arc, OnceLock};
    use std::time::{Duration, Instant};

    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
    use winit::window::{Window, WindowId};

    use crate::args::{Frames, Info};
    use wine_unixlib::{STATUS_BUFFER_TOO_SMALL, STATUS_INVALID_PARAMETER, STATUS_SUCCESS, STATUS_UNSUCCESSFUL, Status};

    static FRAMES: AtomicU32 = AtomicU32::new(0);
    static ENDED: AtomicBool = AtomicBool::new(false);
    /// The adapter ("name (backend)"), or the window's error.
    static INFO: OnceLock<String> = OnceLock::new();
    static PROXY: OnceLock<EventLoopProxy<()>> = OnceLock::new();

    struct Gpu {
        window: Arc<Window>,
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
    }

    #[derive(Default)]
    struct App(Option<Gpu>);

    fn fail(event_loop: &ActiveEventLoop, e: String) {
        let _ = INFO.set(format!("error: {e}"));
        event_loop.exit();
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.0.is_some() {
                return;
            }
            let window = match event_loop.create_window(Window::default_attributes().with_title("wine-unixlib-window").with_inner_size(winit::dpi::PhysicalSize::new(320, 240))) {
                Ok(w) => Arc::new(w),
                Err(e) => return fail(event_loop, format!("window: {e}")),
            };
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
            let surface = match instance.create_surface(window.clone()) {
                Ok(s) => s,
                Err(e) => return fail(event_loop, format!("surface: {e}")),
            };
            let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                apply_limit_buckets: false,
            })) {
                Ok(a) => a,
                Err(e) => return fail(event_loop, format!("adapter: {e}")),
            };
            let (device, queue) = match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())) {
                Ok(d) => d,
                Err(e) => return fail(event_loop, format!("device: {e}")),
            };
            let size = window.inner_size();
            let Some(config) = surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)) else {
                return fail(event_loop, "the adapter cannot present to the window".into());
            };
            surface.configure(&device, &config);
            let _ = INFO.set(format!("{} ({:?})", adapter.get_info().name, adapter.get_info().backend));
            window.request_redraw();
            self.0 = Some(Gpu { window, surface, device, queue, config });
        }

        fn user_event(&mut self, event_loop: &ActiveEventLoop, (): ()) {
            event_loop.exit();
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
            let Some(gpu) = &mut self.0 else { return };
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                    gpu.config.width = size.width;
                    gpu.config.height = size.height;
                    gpu.surface.configure(&gpu.device, &gpu.config);
                }
                WindowEvent::RedrawRequested => {
                    let texture = match gpu.surface.get_current_texture() {
                        wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                        _ => {
                            gpu.surface.configure(&gpu.device, &gpu.config);
                            gpu.window.request_redraw();
                            return;
                        }
                    };
                    let view = texture.texture.create_view(&Default::default());
                    let n = FRAMES.load(Ordering::Relaxed);
                    let shade = f64::from(n % 120) / 120.0;
                    let mut encoder = gpu.device.create_command_encoder(&Default::default());
                    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: shade, g: 0.3, b: 1.0 - shade, a: 1.0 }), store: wgpu::StoreOp::Store },
                        })],
                        ..Default::default()
                    });
                    gpu.queue.submit([encoder.finish()]);
                    gpu.window.pre_present_notify();
                    gpu.queue.present(texture);
                    FRAMES.fetch_add(1, Ordering::Release);
                    gpu.window.request_redraw();
                }
                _ => {}
            }
        }
    }

    fn run() {
        let mut builder = EventLoop::builder();
        #[cfg(windows)]
        winit::platform::windows::EventLoopBuilderExtWindows::with_any_thread(&mut builder, true);
        #[cfg(target_os = "linux")]
        winit::platform::wayland::EventLoopBuilderExtWayland::with_any_thread(&mut builder, true);
        match builder.build() {
            Ok(event_loop) => {
                let _ = PROXY.set(event_loop.create_proxy());
                if let Err(e) = event_loop.run_app(&mut App::default()) {
                    let _ = INFO.set(format!("error: event loop: {e}"));
                }
            }
            Err(e) => {
                let _ = INFO.set(format!("error: event loop: {e}"));
            }
        }
        ENDED.store(true, Ordering::Release);
    }

    pub fn open(_: &mut Frames) -> Status {
        static STARTED: std::sync::Once = std::sync::Once::new();
        let mut status = STATUS_SUCCESS;
        STARTED.call_once(|| {
            if wine_unixlib::unix::spawn_native("wine-unixlib-window", run).is_err() {
                status = STATUS_UNSUCCESSFUL;
            }
        });
        status
    }

    /// Polls (a millisecond apart) until `done`, the window's end, or
    /// `timeout_ms`; fills in `args`.
    fn wait_for(args: &mut Frames, done: impl Fn() -> bool) -> Status {
        let started = Instant::now();
        let timeout = Duration::from_millis(args.timeout_ms.into());
        let reached = loop {
            if done() {
                break true;
            }
            if ENDED.load(Ordering::Acquire) || started.elapsed() >= timeout {
                break done();
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        args.waited_ms = started.elapsed().as_millis() as u32;
        args.frames = FRAMES.load(Ordering::Acquire);
        if reached { STATUS_SUCCESS } else { STATUS_UNSUCCESSFUL }
    }

    pub fn wait(args: &mut Frames) -> Status {
        let want = args.frames;
        wait_for(args, || FRAMES.load(Ordering::Acquire) >= want)
    }

    pub fn info(args: &mut Info) -> Status {
        let text = INFO.get().map_or("pending", String::as_str).as_bytes();
        args.len = text.len() as u32;
        if text.len() > args.out.len as usize {
            return STATUS_BUFFER_TOO_SMALL;
        }
        match wine_unixlib::unix::write(args.out, text) {
            Ok(()) => STATUS_SUCCESS,
            Err(_) => STATUS_INVALID_PARAMETER,
        }
    }

    pub fn close(args: &mut Frames) -> Status {
        if let Some(proxy) = PROXY.get() {
            let _ = proxy.send_event(());
        }
        wait_for(args, || ENDED.load(Ordering::Acquire))
    }
}

#[cfg(not(target_arch = "x86"))]
use native::{close, info, open, wait};

#[cfg(not(target_arch = "x86"))]
wine_unixlib::unix_call_table![open, wait, info, close];
