//! Bounded backdrop copies and edge draws for magnifier cameras.
use super::*;
use tile_render_shared::LensParams;

pub(super) struct State {
    pub format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    params: [wgpu::Buffer; CONTROL_SLOTS],
    snapshot: Option<wgpu::Texture>,
}

impl State {
    pub fn new(device: &wgpu::Device, shader: &wgpu::ShaderModule, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lens backdrop"),
            entries: &[texture_entry(0), wgpu::BindGroupLayoutEntry {
                binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: NonZeroU64::new(size_of::<LensParams>() as u64) }, count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("lens"), bind_group_layouts: &[Some(&layout)], ..Default::default() });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("polished lens rim"), layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: shader, entry_point: Some("fullscreen_vs"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some("lens_fs"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
        });
        let params = std::array::from_fn(|_| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lens parameters"), size: PARAMS_STRIDE * MAX_VIEWS as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        }));
        Self { format, layout, pipeline, params, snapshot: None }
    }

    pub fn bytes(&self) -> u64 {
        self.params.iter().map(|p| p.size()).sum::<u64>() + self.snapshot.as_ref().map_or(0, |t|
            u64::from(t.width()) * u64::from(t.height()) * if t.format() == wgpu::TextureFormat::Rgba16Float { 8 } else { 4 })
    }

    pub fn snapshot(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, canvas: &wgpu::Texture, camera: &Camera, slot: usize, view: usize, bits: u32, quantized_float: u32) -> Option<LensParams> {
        let lens = camera.lens?;
        let pad = (lens.rim * 3.0).ceil() as i32 + 1;
        let (x, y) = ((camera.origin.0 - pad).max(0), (camera.origin.1 - pad).max(0));
        let (r, b) = ((camera.origin.0 + camera.viewport.0 as i32 + pad).min(canvas.width() as i32), (camera.origin.1 + camera.viewport.1 as i32 + pad).min(canvas.height() as i32));
        if r <= x || b <= y { return None; }
        let size = wgpu::Extent3d { width: (r - x) as u32, height: (b - y) as u32, depth_or_array_layers: 1 };
        if self.snapshot.as_ref().is_none_or(|t| t.width() < size.width || t.height() < size.height || t.format() != canvas.format()) {
            self.snapshot = Some(device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bounded lens backdrop"), size, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: canvas.format(),
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
            }));
        }
        let source = wgpu::TexelCopyTextureInfo { origin: wgpu::Origin3d { x: x as u32, y: y as u32, z: 0 }, ..canvas.as_image_copy() };
        encoder.copy_texture_to_texture(source, self.snapshot.as_ref().unwrap().as_image_copy(), size);
        let params = LensParams { x: camera.origin.0 as f32, y: camera.origin.1 as f32, width: camera.viewport.0 as f32, height: camera.viewport.1 as f32,
            radius: lens.radius, rim: lens.rim, glass: u32::from(lens.glass), bits, snapshot_x: x, snapshot_y: y, snapshot_w: r - x, snapshot_h: b - y, quantized_float, _pad0: 0, _pad1: 0, _pad2: 0 };
        queue.write_buffer(&self.params[slot], view as u64 * PARAMS_STRIDE, bytemuck::bytes_of(&params));
        Some(params)
    }

    pub fn draw(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, canvas: &wgpu::TextureView, camera: &Camera, p: LensParams, slot: usize, view: usize, timing: Option<wgpu::RenderPassTimestampWrites<'_>>) {
        let backdrop = self.snapshot.as_ref().unwrap().create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("lens backdrop"), layout: &self.layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&backdrop) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &self.params[slot], offset: 0, size: NonZeroU64::new(size_of::<LensParams>() as u64) }) },
        ] });
        let mut pass = begin_pass(encoder, canvas, wgpu::LoadOp::Load, timing);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[(view as u64 * PARAMS_STRIDE) as u32]);
        let clip = camera.target_clip.unwrap_or([p.snapshot_x, p.snapshot_y, p.snapshot_x + p.snapshot_w, p.snapshot_y + p.snapshot_h]);
        for [x, y, w, h] in edge_rects(camera) {
            let (l, t) = (x.max(clip[0]).max(p.snapshot_x), y.max(clip[1]).max(p.snapshot_y));
            let (r, b) = ((x + w).min(clip[2]).min(p.snapshot_x + p.snapshot_w), (y + h).min(clip[3]).min(p.snapshot_y + p.snapshot_h));
            if r > l && b > t { pass.set_scissor_rect(l as u32, t as u32, (r - l) as u32, (b - t) as u32); pass.draw(0..3, 0..1); }
        }
    }
}

/// Four narrow straight edges and four corner squares, with no overlap.
fn edge_rects(c: &Camera) -> [[i32; 4]; 8] {
    let l = c.lens.unwrap();
    let (x, y, w, h) = (c.origin.0, c.origin.1, c.viewport.0 as i32, c.viewport.1 as i32);
    let a = (l.radius.max(l.rim).ceil() as i32 + 1).min(w.min(h) / 2);
    let e = (l.rim.ceil() as i32 + 1).min(a);
    [[x+a,y,w-2*a,e], [x+a,y+h-e,w-2*a,e], [x,y+a,e,h-2*a], [x+w-e,y+a,e,h-2*a],
     [x,y,a,a], [x+w-a,y,a,a], [x,y+h-a,a,a], [x+w-a,y+h-a,a,a]]
}
