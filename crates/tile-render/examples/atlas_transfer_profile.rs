//! First-use atlas upload latency, including work wgpu inserts before its copy.
//!
//! TILE_RENDER_GPU_TIMING=1 cargo run --release -p tile-render
//!   --example atlas_transfer_profile -- 2048 4096 8192 16384
//!
//! This manual diagnostic waits for readback. The live transfer API does not.

use std::{error::Error, time::Instant};
use tile_render::{Atlas, Headless, RgbaImage, TileRenderer};
use tile_render::renderer::AtlasTransferSource;

const BUDGET: u64 = 4 << 20;

fn stamp(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    queries: &wgpu::QuerySet,
    index: u32,
) {
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("atlas upload timestamp boundary"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: Some(wgpu::RenderPassTimestampWrites {
            query_set: queries,
            beginning_of_pass_write_index: Some(index),
            end_of_pass_write_index: None,
        }),
        occlusion_query_set: None,
        multiview_mask: None,
    }));
}

fn main() -> Result<(), Box<dyn Error>> {
    let gpu = Headless::new()?;
    if !gpu.device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
        return Err("set TILE_RENDER_GPU_TIMING=1 on an adapter with timestamp queries".into());
    }
    println!("adapter: {:?}", gpu.adapter.get_info());
    println!("size,sample,begin_ms,advance_ms,charged_bytes,queue_interval_ms");
    let args: Vec<_> = std::env::args().skip(1).collect();
    let sizes: Vec<u32> = if args.is_empty() { vec![2048, 4096] }
        else { args.iter().map(|v| v.parse()).collect::<Result<_, _>>()? };
    let queries = gpu.device.create_query_set(&wgpu::QuerySetDescriptor {
        label: Some("first atlas upload timing"),
        ty: wgpu::QueryType::Timestamp,
        count: 2,
    });
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("atlas upload timestamp target"),
        size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let resolve = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("atlas upload timestamps"),
        size: 256,
        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("atlas upload timestamp readback"),
        size: 16,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    for size in sizes {
        if size == 0 || size > gpu.device.limits().max_texture_dimension_2d {
            return Err(format!("page size {size} is outside this device's texture limits").into());
        }
        let atlas = Atlas {
            page_size: size,
            pages: vec![RgbaImage::new(size, size)],
            index_pages: Vec::new(),
            sprites: Vec::new(),
            palettes: tile_render::atlas::no_palettes(),
            named_palettes: 1,
        };
        for sample in 1..=3 {
            let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
            let start = Instant::now();
            renderer.begin_atlas_transfer(&gpu.device, AtlasTransferSource::Whole(&atlas), (0, 0))?;
            let begin = start.elapsed().as_secs_f64() * 1000.0;
            let mut before = gpu.device.create_command_encoder(&Default::default());
            stamp(&mut before, &view, &queries, 0);
            gpu.queue.submit([before.finish()]);
            let start = Instant::now();
            let progress = renderer.advance_atlas_transfer(&gpu.device, &gpu.queue, AtlasTransferSource::Whole(&atlas), BUDGET)?;
            let advance = start.elapsed().as_secs_f64() * 1000.0;
            let mut after = gpu.device.create_command_encoder(&Default::default());
            stamp(&mut after, &view, &queries, 1);
            after.resolve_query_set(&queries, 0..2, &resolve, 0);
            after.copy_buffer_to_buffer(&resolve, 0, &readback, 0, 16);
            gpu.queue.submit([after.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            readback.slice(..).map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
            gpu.device.poll(wgpu::PollType::wait_indefinitely())?;
            rx.recv()??;
            let interval = {
                let mapped = readback.slice(..).get_mapped_range()?;
                let ticks: &[u64] = bytemuck::cast_slice(&mapped);
                ticks[1].saturating_sub(ticks[0]) as f64 * f64::from(gpu.queue.get_timestamp_period()) / 1e6
            };
            readback.unmap();
            println!("{size},{sample},{begin:.3},{advance:.3},{},{interval:.3}", progress.bytes);
            renderer.cancel_atlas_transfer(&gpu.queue);
            gpu.device.poll(wgpu::PollType::wait_indefinitely())?;
            renderer.poll_atlas_transfers(&gpu.device);
        }
    }
    Ok(())
}
