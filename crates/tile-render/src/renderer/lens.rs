//! Bounded backdrop copies and edge draws for magnifier cameras.
use super::*;
use std::cell::RefCell;
use tile_render_shared::LensParams;

pub(super) struct State {
    layout: wgpu::BindGroupLayout,
    shader: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    /// The rim pipeline for each format it has been drawn into: the
    /// internal canvas (`Rgba8Unorm`, `Rgba16Float`), the caller's target
    /// when drawn directly, and the window a scaled picture is shown on
    /// (`Bgra8Unorm` or `Rgba8Unorm`).
    pipelines: RefCell<Vec<(wgpu::TextureFormat, wgpu::RenderPipeline)>>,
    params: [wgpu::Buffer; CONTROL_SLOTS],
    // Before-camera backdrop, clear face, then shaded rim. The last image
    // lets the window pass detect pixels covered by subsequent interface art.
    snapshots: Vec<Option<[wgpu::Texture; 3]>>,
    active: Vec<(usize, Camera, LensParams)>,
}

impl State {
    pub fn new(device: &wgpu::Device, shader: &wgpu::ShaderModule) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lens backdrop"),
            entries: &[texture_entry(0), texture_entry(2), texture_entry(3), texture_entry(4), wgpu::BindGroupLayoutEntry {
                binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: NonZeroU64::new(size_of::<LensParams>() as u64) }, count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("lens"), bind_group_layouts: &[Some(&layout)], ..Default::default() });
        let params = std::array::from_fn(|_| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lens parameters"), size: PARAMS_STRIDE * (MAX_VIEWS * 2) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        }));
        Self { layout, shader: shader.clone(), pipeline_layout, pipelines: RefCell::new(Vec::new()), params, snapshots: Vec::new(), active: Vec::new() }
    }

    /// The rim pipeline drawing into `format`, made when first needed.
    fn pipeline(&self, device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        if let Some((_, p)) = self.pipelines.borrow().iter().find(|(f, _)| *f == format) { return p.clone(); }
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("polished lens rim {format:?}")), layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState { module: &self.shader, entry_point: Some("fullscreen_vs"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState { module: &self.shader, entry_point: Some("lens_fs"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
        });
        self.pipelines.borrow_mut().push((format, pipeline.clone()));
        pipeline
    }

    pub fn bytes(&self) -> u64 {
        self.params.iter().map(|p| p.size()).sum::<u64>() + self.snapshots.iter().flatten().flatten().map(|t|
            u64::from(t.width()) * u64::from(t.height()) * if t.format() == wgpu::TextureFormat::Rgba16Float { 8 } else { 4 }).sum::<u64>()
    }

    pub fn begin_frame(&mut self, keep_before: usize) {
        self.active.retain(|(view, _, _)| *view < keep_before);
    }

    pub fn snapshot(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, canvas: &wgpu::Texture, camera: &Camera, slot: usize, view: usize, bits: u32, quantized_float: u32) -> Option<LensParams> {
        let lens = camera.lens?;
        let pad = (lens.rim * 0.28).ceil() as i32 + 1;
        let (x, y) = ((camera.origin.0 - pad).max(0), (camera.origin.1 - pad).max(0));
        let (r, b) = ((camera.origin.0 + camera.viewport.0 as i32 + pad).min(canvas.width() as i32), (camera.origin.1 + camera.viewport.1 as i32 + pad).min(canvas.height() as i32));
        if r <= x || b <= y { return None; }
        let size = wgpu::Extent3d { width: (r - x) as u32, height: (b - y) as u32, depth_or_array_layers: 1 };
        self.snapshots.resize_with(self.snapshots.len().max(view + 1), || None);
        if self.snapshots[view].as_ref().is_none_or(|t| t[0].width() < size.width || t[0].height() < size.height || t[0].format() != canvas.format()) {
            self.snapshots[view] = Some(std::array::from_fn(|_| device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bounded lens backdrop"), size, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: canvas.format(),
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
            })));
        }
        let source = wgpu::TexelCopyTextureInfo { origin: wgpu::Origin3d { x: x as u32, y: y as u32, z: 0 }, ..canvas.as_image_copy() };
        encoder.copy_texture_to_texture(source, self.snapshots[view].as_ref().unwrap()[0].as_image_copy(), size);
        let params = LensParams { x: camera.origin.0 as f32, y: camera.origin.1 as f32, width: camera.viewport.0 as f32, height: camera.viewport.1 as f32,
            radius: lens.radius, rim: lens.rim, glass: u32::from(lens.glass), bits, snapshot_x: x, snapshot_y: y, snapshot_w: r - x, snapshot_h: b - y, quantized_float,
            sample_scale: 1.0, output_bits: bits, ..Default::default() };
        queue.write_buffer(&self.params[slot], view as u64 * PARAMS_STRIDE, bytemuck::bytes_of(&params));
        self.active.push((view, *camera, params));
        Some(params)
    }

    pub fn capture(&self, encoder: &mut wgpu::CommandEncoder, canvas: &wgpu::Texture, p: LensParams, view: usize, shaded: bool) {
        let source = wgpu::TexelCopyTextureInfo { origin: wgpu::Origin3d { x: p.snapshot_x as u32, y: p.snapshot_y as u32, z: 0 }, ..canvas.as_image_copy() };
        encoder.copy_texture_to_texture(source, self.snapshots[view].as_ref().unwrap()[if shaded { 2 } else { 1 }].as_image_copy(),
            wgpu::Extent3d { width: p.snapshot_w as u32, height: p.snapshot_h as u32, depth_or_array_layers: 1 });
    }

    /// Re-shade at physical window resolution. The art remains boxed, but the
    /// glass, dispersion and silhouette coverage do not become 2x2 blocks.
    pub fn draw_presented(&self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, target: &wgpu::Texture, composed: &wgpu::Texture, present: Present, slot: usize, timer: Option<&crate::timing::GpuTimer>) {
        let scale = present.zoom.0.max(1) as f32 / present.zoom.1.max(1) as f32;
        if scale == 1.0 && present.at == (0, 0) { return; }
        let canvas = target.create_view(&Default::default());
        let pipeline = self.pipeline(device, target.format());
        for &(view, camera, mut p) in &self.active {
            p.x = p.x * scale + present.at.0 as f32;
            p.y = p.y * scale + present.at.1 as f32;
            p.width *= scale; p.height *= scale; p.radius *= scale; p.rim *= scale;
            p.sample_scale = 1.0 / scale;
            p.sample_x = -present.at.0 as f32 / scale;
            p.sample_y = -present.at.1 as f32 / scale;
            p.output_bits = 0;
            p.check_occlusion = 1;
            let transform = |v: i32, at: i32| (v as f32 * scale).floor() as i32 + at;
            let clip = camera.target_clip.unwrap_or([0, 0, present.canvas.0 as i32, present.canvas.1 as i32]);
            let camera = Camera { origin: (p.x.floor() as i32, p.y.floor() as i32), viewport: (p.width.ceil() as u32, p.height.ceil() as u32),
                lens: Some(crate::Lens { radius: p.radius, rim: p.rim, glass: p.glass != 0 }),
                target_clip: Some([transform(clip[0], present.at.0).max(0), transform(clip[1], present.at.1).max(0),
                    transform(clip[2], present.at.0).min(target.width() as i32), transform(clip[3], present.at.1).min(target.height() as i32)]), ..camera };
            queue.write_buffer(&self.params[slot], (MAX_VIEWS + view) as u64 * PARAMS_STRIDE, bytemuck::bytes_of(&p));
            self.draw_with(device, encoder, &pipeline, &canvas, &camera, p, slot, view, Some(composed), timer.and_then(|t| t.pass(|| format!("v{view} window lens rim"))));
        }
    }

    /// Draws the rim of `view` into `canvas`, a view of a texture of `format`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, canvas: &wgpu::TextureView, format: wgpu::TextureFormat, camera: &Camera, p: LensParams, slot: usize, view: usize, timing: Option<wgpu::RenderPassTimestampWrites<'_>>) {
        self.draw_with(device, encoder, &self.pipeline(device, format), canvas, camera, p, slot, view, None, timing);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_with(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, pipeline: &wgpu::RenderPipeline, canvas: &wgpu::TextureView, camera: &Camera, p: LensParams, slot: usize, view: usize, composed: Option<&wgpu::Texture>, timing: Option<wgpu::RenderPassTimestampWrites<'_>>) {
        let presented = composed.is_some();
        let snapshots = self.snapshots[view].as_ref().unwrap();
        let backdrop = snapshots[0].create_view(&Default::default());
        let face = snapshots[1].create_view(&Default::default());
        let shaded = snapshots[2].create_view(&Default::default());
        let composed = composed.unwrap_or(&snapshots[0]).create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("lens backdrop"), layout: &self.layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&backdrop) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&face) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&composed) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&shaded) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &self.params[slot], offset: 0, size: NonZeroU64::new(size_of::<LensParams>() as u64) }) },
        ] });
        let mut pass = begin_pass(encoder, canvas, wgpu::LoadOp::Load, timing);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group, &[((view + if presented { MAX_VIEWS } else { 0 }) as u64 * PARAMS_STRIDE) as u32]);
        let sx = |x: i32| ((x as f32 - p.sample_x) / p.sample_scale).floor() as i32;
        let sy = |y: i32| ((y as f32 - p.sample_y) / p.sample_scale).floor() as i32;
        let bounds = [sx(p.snapshot_x), sy(p.snapshot_y), sx(p.snapshot_x + p.snapshot_w), sy(p.snapshot_y + p.snapshot_h)];
        let clip = camera.target_clip.unwrap_or(bounds);
        let guard = if presented { (1.0 / p.sample_scale).ceil() as i32 } else { 0 };
        for [x, y, w, h] in edge_rects(camera, guard) {
            let (l, t) = (x.max(clip[0]).max(bounds[0]), y.max(clip[1]).max(bounds[1]));
            let (r, b) = ((x + w).min(clip[2]).min(bounds[2]), (y + h).min(clip[3]).min(bounds[3]));
            if r > l && b > t { pass.set_scissor_rect(l as u32, t as u32, (r - l) as u32, (b - t) as u32); pass.draw(0..3, 0..1); }
        }
    }
}

/// Four narrow straight edges and four corner squares, with no overlap.
fn edge_rects(c: &Camera, guard: i32) -> [[i32; 4]; 8] {
    let l = c.lens.unwrap();
    let (x, y, w, h) = (c.origin.0, c.origin.1, c.viewport.0 as i32, c.viewport.1 as i32);
    let halo = if l.glass { (l.rim * 0.28).ceil() as i32 } else { 0 };
    let a = (l.radius.max(l.rim).ceil() as i32 + 1 + guard).min(w.min(h) / 2);
    let e = (l.rim.ceil() as i32 + 1 + guard).min(a);
    [[x+a,y-halo,w-2*a,e+halo], [x+a,y+h-e,w-2*a,e+halo],
     [x-halo,y+a,e+halo,h-2*a], [x+w-e,y+a,e+halo,h-2*a],
     [x-halo,y-halo,a+halo,a+halo], [x+w-a,y-halo,a+halo,a+halo],
     [x-halo,y+h-a,a+halo,a+halo], [x+w-a,y+h-a,a+halo,a+halo]]
}
