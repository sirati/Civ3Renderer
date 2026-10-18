//! Device setup and offscreen rendering.

use crate::atlas::RgbaImage;
use crate::camera::Camera;
use crate::renderer::TileRenderer;

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("no GPU adapter{}", .0.as_ref().map(|f| format!(" matching {f:?}")).unwrap_or_default())]
    NoAdapter(Option<String>),
    #[error(transparent)]
    RequestDevice(#[from] wgpu::RequestDeviceError),
}

/// Picks an adapter. `filter` (or else the `TILE_RENDER_ADAPTER` environment
/// variable) selects by case-insensitive substring of the adapter name, e.g.
/// "radeon" or "llvmpipe". "nvidia" and "intel" also match their PCI vendor
/// IDs, so an NVIDIA adapter named "Quadro P2000" is recognized. Without
/// either, wgpu's default choice is used.
/// With a `surface`, only an adapter that can present to it: if the named
/// ones cannot (e.g. a discrete GPU not driving the display, whose driver
/// cannot hand its pictures to the compositor), the default choice for the
/// surface instead, logged as an error. No adapter of that name at all
/// (e.g. its driver did not load): the default one too, with a warning.
pub async fn select_adapter(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
    filter: Option<&str>,
) -> Result<wgpu::Adapter, GpuError> {
    let filter = filter.map(str::to_owned).or_else(|| std::env::var("TILE_RENDER_ADAPTER").ok());
    if let Some(f) = &filter {
        let needle = f.to_lowercase();
        let (mut seen, mut unpresentable) = (Vec::new(), Vec::new());
        for adapter in instance.enumerate_adapters(wgpu::Backends::all()).await {
            let info = adapter.get_info();
            let name = format!("{} ({:?})", info.name, info.backend);
            if !adapter_matches(&info.name, info.vendor, &needle) {
                seen.push(name);
            } else if surface.is_none_or(|s| adapter.is_surface_supported(s)) {
                log::info!("using adapter {name}");
                return Ok(adapter);
            } else {
                unpresentable.push(name);
            }
        }
        if unpresentable.is_empty() {
            log::warn!(
                "no adapter matches {f:?} (its driver is not loaded, or there is no such GPU); there are: {}; using the default adapter",
                if seen.is_empty() { "none".into() } else { seen.join(", ") }
            );
            return default_adapter(instance, surface).await;
        }
        log::error!(
            "{} {} {f:?} but cannot present to this window; using the default adapter for it instead (of: {})",
            unpresentable.join(", "),
            if unpresentable.len() == 1 { "matches" } else { "match" },
            if seen.is_empty() { "none".into() } else { seen.join(", ") }
        );
    }
    default_adapter(instance, surface).await
}

// Vendor names are launcher choices, while all other filters retain the
// existing device-name substring behavior. In particular "radv" must name
// that driver, rather than accepting every AMD backend by vendor alone.
fn adapter_matches(name: &str, vendor: u32, needle: &str) -> bool {
    name.to_lowercase().contains(needle)
        || matches!((needle, vendor), ("nvidia", 0x10de) | ("intel", 0x8086))
}

/// wgpu's default adapter (low power; with a `surface`, one that presents
/// to it), whatever `TILE_RENDER_ADAPTER` says: e.g. when the one named
/// cannot show a window after all.
pub async fn default_adapter(instance: &wgpu::Instance, surface: Option<&wgpu::Surface<'_>>) -> Result<wgpu::Adapter, GpuError> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: surface,
            apply_limit_buckets: false,
        })
        .await
        .map_err(|_| GpuError::NoAdapter(None))?;
    log::info!("using adapter {} ({:?})", adapter.get_info().name, adapter.get_info().backend);
    Ok(adapter)
}

/// The largest texture side (`max_texture_dimension_2d`) of the adapter
/// [`select_adapter`] picks without a surface: what a device
/// [`request_device`] makes on it allows, e.g. for the atlas pages
/// (`render_desc::page_size`). Probed once per process; `None` without an
/// adapter.
pub fn max_texture_dimension() -> Option<u32> {
    static MAX: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
    *MAX.get_or_init(|| {
        let instance = wgpu::Instance::new(instance_descriptor());
        match pollster::block_on(select_adapter(&instance, None, None)) {
            Ok(adapter) => {
                let max = adapter.limits().max_texture_dimension_2d;
                log::info!("{}: textures up to {max}×{max}", adapter.get_info().name);
                Some(max)
            }
            Err(e) => {
                log::warn!("no texture size limit known: {e}");
                None
            }
        }
    })
}

/// A device on `adapter`; with timestamp queries where the adapter has them
/// and GPU timing is asked for (`crate::timing::requested`).
pub async fn request_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), GpuError> {
    let timing = crate::timing::requested() && adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
    Ok(adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("tile-render"),
            required_features: if timing { wgpu::Features::TIMESTAMP_QUERY } else { wgpu::Features::empty() },
            // (As many atlas pages as the adapter allows: an atlas grows.)
            required_limits: wgpu::Limits { max_texture_array_layers: adapter.limits().max_texture_array_layers, ..wgpu::Limits::default().using_resolution(adapter.limits()) },
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        })
        .await?)
}

/// The instance settings: backends from `WGPU_BACKEND`, and no validation
/// or debug callbacks unless asked for (`WGPU_DEBUG`, `WGPU_VALIDATION`):
/// the renderer often runs inside other programs, on machines whose
/// graphics stack it cannot vouch for.
pub fn instance_descriptor() -> wgpu::InstanceDescriptor {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    desc.flags = wgpu::InstanceFlags::empty().with_env();
    desc
}

/// A device without a window, for tests and screenshot tools.
pub struct Headless {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Headless {
    pub fn new() -> Result<Self, GpuError> {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(instance_descriptor());
            let adapter = select_adapter(&instance, None, None).await?;
            let (device, queue) = request_device(&adapter).await?;
            Ok(Self { adapter, device, queue })
        })
    }

    /// Renders one frame offscreen and reads it back.
    pub fn render(&self, renderer: &mut TileRenderer, camera: &Camera, frame: u32, clear: [u8; 4]) -> RgbaImage {
        self.render_views(renderer, std::slice::from_ref(camera), camera.viewport, frame, clear)
    }

    /// Renders several views into an offscreen target of `size` (see
    /// `TileRenderer::render_views`).
    pub fn render_views(&self, renderer: &mut TileRenderer, cameras: &[Camera], size: (u32, u32), frame: u32, clear: [u8; 4]) -> RgbaImage {
        self.render_composed(renderer, cameras, &[], size, frame, clear)
    }

    /// Renders views and screen copies into an offscreen image (see
    /// `TileRenderer::render_composed`).
    pub fn render_composed(
        &self,
        renderer: &mut TileRenderer,
        cameras: &[Camera],
        copies: &[crate::ScreenCopy],
        size: (u32, u32),
        frame: u32,
        clear: [u8; 4],
    ) -> RgbaImage {
        self.render_presented(renderer, cameras, copies, crate::Present::unscaled(size), frame, clear)
    }

    /// Renders views and screen copies into a picture shown on an offscreen
    /// image of `present.target` (see `TileRenderer::render_presented`).
    pub fn render_presented(
        &self,
        renderer: &mut TileRenderer,
        cameras: &[Camera],
        copies: &[crate::ScreenCopy],
        present: crate::Present,
        frame: u32,
        clear: [u8; 4],
    ) -> RgbaImage {
        self.render_layered(renderer, cameras, cameras.len(), copies, present, frame, clear)
    }

    /// Renders views, the last from `overlay_from` on overlays, into a
    /// picture shown on an offscreen image of `present.target` (see
    /// `TileRenderer::render_layered`).
    #[allow(clippy::too_many_arguments)]
    pub fn render_layered(
        &self,
        renderer: &mut TileRenderer,
        cameras: &[Camera],
        overlay_from: usize,
        copies: &[crate::ScreenCopy],
        present: crate::Present,
        frame: u32,
        clear: [u8; 4],
    ) -> RgbaImage {
        let (w, h) = present.target;
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        renderer.render_layered(&self.device, &self.queue, &mut encoder, &target, cameras, overlay_from, copies, frame, clear, present);
        read_texture(&self.device, &self.queue, encoder, &target)
    }
}

pub fn color([r, g, b, a]: [u8; 4]) -> wgpu::Color {
    let c = |v: u8| v as f64 / 255.0;
    wgpu::Color { r: c(r), g: c(g), b: c(b), a: c(a) }
}

/// Copies an `Rgba8Unorm`/`Bgra8Unorm` texture back to the CPU (as RGBA),
/// submitting `encoder` first.
pub fn read_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    texture: &wgpu::Texture,
) -> RgbaImage {
    let (w, h) = (texture.width(), texture.height());
    let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |r| r.expect("map readback buffer"));
    device.poll(wgpu::PollType::wait_indefinitely()).expect("poll device");
    let bgra = texture.format() == wgpu::TextureFormat::Bgra8Unorm;
    let data = buffer.slice(..).get_mapped_range().expect("readback buffer mapped");
    RgbaImage::from_fn(w, h, |x, y| {
        let i = (y * row + x * 4) as usize;
        let p = [data[i], data[i + 1], data[i + 2], data[i + 3]];
        if bgra { [p[2], p[1], p[0], p[3]] } else { p }
    })
}

#[cfg(test)]
mod tests {
    use super::adapter_matches;

    #[test]
    fn launcher_vendor_names_recognize_devices_without_vendor_in_the_name() {
        assert!(adapter_matches("Quadro P2000", 0x10de, "nvidia"));
        assert!(adapter_matches("GeForce GTX 1060", 0x10de, "nvidia"));
        assert!(adapter_matches("UHD Graphics 630", 0x8086, "intel"));
        assert!(!adapter_matches("UHD Graphics 630", 0x8086, "nvidia"));
        assert!(!adapter_matches("Quadro P2000", 0x10de, "intel"));
    }

    #[test]
    fn device_name_substrings_and_radv_driver_requests_keep_their_meaning() {
        assert!(adapter_matches("Quadro P2000", 0x10de, "p2000"));
        assert!(adapter_matches("llvmpipe (LLVM 20)", 0x10005, "llvmpipe"));
        assert!(adapter_matches("AMD Radeon Graphics (RADV GFX1103_R1)", 0x1002, "radv"));
        assert!(!adapter_matches("AMD Radeon Graphics", 0x1002, "radv"));
        assert!(!adapter_matches("Quadro P2000", 0x10de, "radv"));
        assert!(!adapter_matches("UHD Graphics 630", 0x8086, "radv"));
    }
}
