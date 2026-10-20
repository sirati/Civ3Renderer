//! Exhaustive source coverage/tint bytes, integer blend oracle, and byte captures
//! for comparing dual-source subpixel sprites with their three-pass fallback.
use std::{fs::File, io::{BufWriter, Write}};
use tile_render::{AtlasBuilder, Blend, Camera, ColorModel, Grid, Headless, IndexImage,
    Layout, PlacedSprite, RgbaImage, SpriteSets, TileRenderer, TileWorld};

fn coverage(x: u8) -> [u8; 4] {
    [x, x.wrapping_mul(73), x.wrapping_mul(151), x.wrapping_mul(37)]
}

fn tint(y: u8, second: bool) -> [u8; 3] {
    let first = [y, y.wrapping_mul(37), 255 - y];
    if second { [255 - first[0], first[2], first[1]] } else { first }
}

fn quantized(c: u8, bits: u32) -> u32 { u32::from(c) >> (8 - bits) }
fn expanded(q: u32, bits: u32) -> u8 {
    ((q << (8 - bits)) | (q >> (2 * bits - 8))) as u8
}

#[test]
fn subpixel_coverage_and_tint_bytes_keep_exact_blends() {
    let gpu = Headless::new().expect("GPU");
    eprintln!("subpixel oracle adapter {:?}, dual-source={} override={:?}",
        gpu.adapter.get_info(), gpu.device.features().contains(wgpu::Features::DUAL_SOURCE_BLENDING),
        std::env::var("TILE_RENDER_SUBPIXEL_DUAL"));
    let mut builder = AtlasBuilder::new(512);
    let rgba = builder.add(&RgbaImage::from_fn(256, 1, |x, _| coverage(x as u8)), (0, 0)).unwrap();
    let index = builder.add_indexed(&IndexImage::from_fn(256, 1, |x, _| x as u8), (0, 0), 1,
        tile_render_shared::remap_range(32, 224)).unwrap();
    let mut atlas = builder.finish();
    atlas.palettes = tile_render::atlas::no_palettes();
    for row in 1..=2 {
        atlas.palettes.extend((0..256).map(|x| {
            let x = if row == 2 { (x as u8).wrapping_mul(91) } else { x as u8 };
            u32::from_le_bytes(coverage(x))
        }));
    }
    atlas.named_palettes = 2;
    let mut sets = SpriteSets::default();
    let rgba = sets.single(rgba);
    let index = sets.single(index);
    let mut world = TileWorld::new(Grid { cols: 1, rows: 1, layout: Layout::Orthogonal,
        step: (1, 1), wrap_x: false, wrap_y: false }, sets);
    let mut renderer = TileRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_atlas(&gpu.device, &gpu.queue, &atlas);
    let mut capture = std::env::var_os("TILE_RENDER_SUBPIXEL_ORACLE")
        .map(|path| BufWriter::new(File::create(path).expect("create capture")));
    let mut checked = 0u64;
    // This explicit diagnostic mode inventories existing wider-depth errors
    // while allowing the separate fallback/dual capture to finish. Normal
    // test runs keep strict integer assertions for every supported depth.
    let wider_inventory = std::env::var("TILE_RENDER_WIDER_ROUNDING_INVENTORY").as_deref() == Ok("1");
    // TileWorld supports direct colour and quantized depths four through seven.
    for bits in [0, 4, 5, 6, 7] {
        let mut wider_mismatches = 0u64;
        world.color = if bits == 0 { ColorModel::Direct } else { ColorModel::Quantized { bits } };
        for material in 0..3 {
            let (set, palette) = if material == 0 { (rgba, 0) } else { (index, material - 1) };
            world.free.clear();
            for y in 0..256u32 {
                for second in [false, true] {
                    // Half the rows exercise two consecutive ordered blends.
                    if second && y % 2 == 0 { continue; }
                    world.free.push(PlacedSprite { screen: true, palette,
                        blend: Blend::Subpixel { color: tint(y as u8, second) },
                        ..PlacedSprite::new(set, 0, y as i32) });
                }
            }
            renderer.set_world(&gpu.device, &world);
            // Every destination level for RGB555 and four-bit colour;
            // critical byte boundaries for the direct and wider models.
            let destinations: Vec<u8> = if bits == 4 || bits == 5 {
                (0..1u32 << bits).map(|q| (q << (8 - bits)) as u8).collect()
            } else { vec![0, 1, 7, 31, 63, 127, 254, 255] };
            for dst in destinations {
                let clear = [dst, dst.wrapping_mul(79), 255 - dst, 37];
                let got = gpu.render(&mut renderer, &Camera::new((256, 256)), 0, clear);
                if let Some(out) = &mut capture { out.write_all(got.as_bytes()).unwrap(); }
                for y in 0..256u32 { for x in 0..256u32 {
                    let got = got.get(x, y);
                    assert_eq!(got[3], clear[3],
                        "target alpha: bits={bits} material={material} dst={dst} x={x} y={y}");
                    if bits != 0 {
                        let index = if material == 2 && (32..224).contains(&x) {
                            (x as u8).wrapping_mul(91)
                        } else { x as u8 };
                        let c = coverage(index);
                        let mut want = [0u8; 4];
                        for channel in 0..3 {
                            let k = (u32::from(c[channel]) * 32 + 127) / 255;
                            let first = (quantized(tint(y as u8, false)[channel], bits) * k
                                + quantized(clear[channel], bits) * (32 - k)) / 32;
                            let final_q = if y % 2 == 0 { first } else {
                                (quantized(tint(y as u8, true)[channel], bits) * k + first * (32 - k)) / 32
                            };
                            want[channel] = expanded(final_q, bits);
                        }
                        want[3] = clear[3];
                        if got != want && bits > 5 && wider_inventory {
                            wider_mismatches += 1;
                            if wider_mismatches <= 4 {
                                eprintln!("OPEN wider integer discrepancy: bits={bits} material={material} dst={dst} x={x} y={y} got={got:?} want={want:?}");
                            }
                        } else {
                            assert_eq!(got, want,
                                "integer blend: bits={bits} material={material} dst={dst} x={x} y={y}");
                        }
                    }
                    checked += 1;
                } }
            }
            // Non-integer zoom, negative origin, clipping and per-axis sample
            // phase are compared byte-for-byte with the fallback capture.
            for zoom in [(2, 1), (3, 2), (2, 3), (1, 3)] {
                let camera = Camera { zoom, origin: (-3, 7),
                    target_clip: Some([11, 13, 299, 277]), hairline: true,
                    ..Camera::new((301, 281)) };
                let got = gpu.render_views(&mut renderer, &[camera], (307, 293), 0, [73, 51, 29, 37]);
                if let Some(out) = &mut capture { out.write_all(got.as_bytes()).unwrap(); }
                checked += u64::from(got.width) * u64::from(got.height);
            }
        }
        if bits > 5 && wider_inventory {
            eprintln!("wider integer inventory bits={bits} mismatched_pixels={wider_mismatches}");
        }
    }
    if let Some(out) = &mut capture { out.flush().unwrap(); }
    eprintln!("subpixel coverage/tint oracle checked {checked} pixels");
}
