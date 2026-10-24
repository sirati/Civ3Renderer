//! Startup uses the same GPU outline renderer as the game. The tiny embedded
//! face is converted to vector contours once, never rasterized on the CPU.
use font8x8::UnicodeFonts;
use std::sync::Arc;
use tile_render::{
    Camera, GlyphShape, Grid, Layout, OutlineGlyph, PlacedSprite, SpriteSetId, SpriteSets,
    TileRenderer, TileWorld,
};
use winit::window::Window;

/// The shader module the startup window draws with (the viewer's; only its
/// tile renderer's shaders are used).
const SHADERS: tile_render::Shaders = tile_render::Shaders::new(viewer_spirv::SPIRV, viewer_spirv::SPIRV_FALLBACK);

pub(super) struct Display {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: TileRenderer,
    world: TileWorld,
}
impl Display {
    pub(super) fn new(window: Arc<Window>) -> Result<Self, String> {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(tile_render::gpu::instance_descriptor());
            let surface = instance
                .create_surface(window.clone())
                .map_err(|e| e.to_string())?;
            let adapter = tile_render::gpu::select_adapter(&instance, Some(&surface), None)
                .await
                .map_err(|e| e.to_string())?;
            let (device, queue) = tile_render::gpu::request_device(&adapter)
                .await
                .map_err(|e| e.to_string())?;
            let size = window.inner_size();
            let mut config = surface
                .get_default_config(&adapter, size.width.max(1), size.height.max(1))
                .ok_or("No GPU presentation format")?;
            config.usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
            config.present_mode = wgpu::PresentMode::AutoVsync;
            config.desired_maximum_frame_latency = 1;
            surface.configure(&device, &config);
            let mut renderer = TileRenderer::new(&device, config.format, SHADERS);
            renderer.set_atlas(
                &device,
                &queue,
                &tile_render::AtlasBuilder::new(16).finish(),
            );
            let mut world = TileWorld::new(
                Grid {
                    cols: 1,
                    rows: 1,
                    layout: Layout::Orthogonal,
                    step: (1, 1),
                    wrap_x: false,
                    wrap_y: false,
                },
                SpriteSets::default(),
            );
            world.fonts = vec![Arc::new(face())];
            renderer.set_world(&device, &world);
            Ok(Self {
                surface,
                device,
                queue,
                config,
                renderer,
                world,
            })
        })
    }
    pub(super) fn paint(
        &mut self,
        window: &Window,
        text: &str,
        scroll: usize,
    ) -> Result<(), String> {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        if (self.config.width, self.config.height) != (size.width, size.height) {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                    _ => return Ok(()),
                }
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            other => return Err(format!("GPU surface unavailable: {other:?}")),
        };
        self.world.free = commands(text, scroll, size.width, size.height, &self.world.fonts[0]);
        self.renderer
            .update_free(&self.device, &self.queue, &self.world);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("startup"),
            });
        self.renderer.render(
            &self.device,
            &self.queue,
            &mut encoder,
            &frame.texture,
            &Camera {
                cells: false,
                ..Camera::new((size.width, size.height))
            },
            0,
            [24, 33, 43, 255],
        );
        self.queue.submit([encoder.finish()]);
        self.renderer.controls_submitted();
        window.pre_present_notify();
        self.queue.present(frame);
        Ok(())
    }
}

fn face() -> tile_render::gpu_text::Face {
    let glyphs = (32u8..127)
        .map(|code| {
            let c = char::from(code);
            let mut curves = Vec::new();
            if let Some(rows) = font8x8::BASIC_FONTS.get(c) {
                for (row, bits) in rows.iter().enumerate() {
                    for col in 0..8 {
                        if bits & (1 << col) != 0 {
                            let (x, y) = (col as f64, 8. - row as f64);
                            let corners = [(x, y), (x, y - 1.), (x + 1., y - 1.), (x + 1., y)];
                            for i in 0..4 {
                                let (a, b) = (corners[i], corners[(i + 1) % 4]);
                                curves.push([a, ((a.0 + b.0) / 2., (a.1 + b.1) / 2.), b]);
                            }
                        }
                    }
                }
            }
            (c, curves, 8)
        })
        .collect::<Vec<_>>();
    tile_render::gpu_text::Face::from_curves(8, &glyphs)
}

fn commands(
    text: &str,
    scroll: usize,
    width: u32,
    height: u32,
    face: &tile_render::gpu_text::Face,
) -> Vec<PlacedSprite> {
    let columns = (width.saturating_sub(40) / 16).max(1) as usize;
    let rows = (height.saturating_sub(20) / 20 + 1) as usize;
    let mut out = Vec::with_capacity((columns * rows).min(16384));
    let (mut x, mut y) = (0usize, 0usize);
    for c in text.chars() {
        if c == '\n' {
            x = 0;
            y = y.saturating_add(1);
            continue;
        }
        if x >= columns {
            x = 0;
            y = y.saturating_add(1);
        }
        if y >= scroll.saturating_add(rows) || out.len() >= 16384 {
            break;
        }
        if y >= scroll {
            if let Some(glyph) = face.glyph(c).or_else(|| face.glyph('?')) {
                out.push(PlacedSprite {
                    screen: true,
                    view: Some(0),
                    text: Some(OutlineGlyph {
                        font: 0,
                        shape: GlyphShape::Glyph(glyph),
                        em64: 16 * 64,
                        advance: 16,
                        bold: false,
                        italic: false,
                        color: [228, 234, 240],
                        key: None,
                    }),
                    ..PlacedSprite::new(
                        SpriteSetId::EMPTY,
                        20 + (x * 16) as i32,
                        36 + ((y - scroll) * 20) as i32,
                    )
                });
            }
        }
        x += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_layout_uploads_only_visible_rows_and_bounds_large_windows() {
        let face = face();
        let text = "A\nB\nC\nD";
        let visible = commands(text, 1, 640, 20, &face);
        assert_eq!(visible.len(), 1);
        assert_eq!(
            visible[0].text.unwrap().shape,
            GlyphShape::Glyph(face.glyph('B').unwrap())
        );
        assert_eq!((visible[0].x, visible[0].y), (20, 36));
        assert!(commands(&"x".repeat(100000), 0, u32::MAX, u32::MAX, &face).len() <= 16384);
        assert!(commands(text, usize::MAX, 640, 300, &face).is_empty());
    }
    #[test]
    fn unsupported_unicode_remains_visible_as_a_gpu_outline() {
        let face = face();
        let visible = commands("é", 0, 640, 300, &face);
        assert_eq!(
            visible[0].text.unwrap().shape,
            GlyphShape::Glyph(face.glyph('?').unwrap())
        );
    }
}

#[cfg(test)]
mod gpu_tests {
    use super::*;
    #[test]
    #[ignore = "requires an explicit GPU lease"]
    fn startup_vector_text_uses_the_existing_shader_path() {
        let gpu = tile_render::Headless::new().expect("GPU shader adapter");
        eprintln!("startup shader adapter: {:?}", gpu.adapter.get_info());
        let mut world = TileWorld::new(
            Grid {
                cols: 1,
                rows: 1,
                layout: Layout::Orthogonal,
                step: (1, 1),
                wrap_x: false,
                wrap_y: false,
            },
            SpriteSets::default(),
        );
        world.fonts = vec![Arc::new(face())];
        world.free = commands("A", 0, 64, 64, &world.fonts[0]);
        let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, SHADERS);
        renderer.set_atlas(
            &gpu.device,
            &gpu.queue,
            &tile_render::AtlasBuilder::new(16).finish(),
        );
        renderer.set_world(&gpu.device, &world);
        let image = gpu.render(
            &mut renderer,
            &Camera {
                cells: false,
                ..Camera::new((64, 64))
            },
            0,
            [24, 33, 43, 255],
        );
        let mut ink = 0;
        for y in 0..64 {
            for x in 0..64 {
                let p = image.get(x, y);
                if p != [24, 33, 43, 255] {
                    ink += 1;
                    assert!(
                        (19..=36).contains(&x) && (19..=37).contains(&y),
                        "startup glyph escaped its layout at {x},{y}"
                    );
                }
            }
        }
        assert!(
            ink > 50,
            "generated outline must produce visible shader-rendered text"
        );
        eprintln!("startup vector text ink pixels: {ink}");
    }
}
