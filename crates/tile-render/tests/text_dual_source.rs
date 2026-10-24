//! The one-draw LCD path must match the independent RGB fallback byte for byte.
use std::sync::Arc;
use tile_render::{AtlasBuilder, Camera, ColorModel, GlyphShape, Grid, Headless, Layout, OutlineGlyph, PlacedSprite, Present, RgbaImage, SpriteSetId, SpriteSets, TileRenderer, TileWorld, ZoomArt};
use tile_render::gpu_text::Face;

fn device(dual: bool) -> Headless {
    pollster::block_on(async {
        let instance=wgpu::Instance::new(tile_render::gpu::instance_descriptor());
        let adapter=tile_render::gpu::select_adapter(&instance,None,None).await.expect("AMD GPU");
        assert!(adapter.features().contains(wgpu::Features::DUAL_SOURCE_BLENDING),"dual-source oracle requires a capable test adapter");
        let (device,queue)=adapter.request_device(&wgpu::DeviceDescriptor {
            label:Some(if dual {"dual LCD oracle"} else {"fallback LCD oracle"}),
            required_features:if dual {wgpu::Features::DUAL_SOURCE_BLENDING} else {wgpu::Features::empty()},
            required_limits:wgpu::Limits {max_texture_array_layers:adapter.limits().max_texture_array_layers,..wgpu::Limits::default().using_resolution(adapter.limits())},
            experimental_features:wgpu::ExperimentalFeatures::disabled(),memory_hints:wgpu::MemoryHints::Performance,trace:wgpu::Trace::Off,
        }).await.unwrap();
        assert_eq!(device.features().contains(wgpu::Features::DUAL_SOURCE_BLENDING),dual);
        Headless {adapter,device,queue}
    })
}

fn font() -> Arc<Face> {
    let path = std::env::var_os("TEST_FONT").expect("TEST_FONT: a real TrueType font (the dev shell sets it)");
    Arc::new(Face::parse(&std::fs::read(path).expect("real font oracle fixture"),0).unwrap())
}

fn world(face: Arc<Face>,color: ColorModel) -> (TileWorld,tile_render::Atlas) {
    let mut atlas=AtlasBuilder::new(512);
    let background=atlas.add(&RgbaImage::from_fn(512,384,|x,y| [x as u8,y as u8,(x^y) as u8,(80+(x+y)%176) as u8]),(0,0)).unwrap();
    let mut sets=SpriteSets::default();
    let bg=sets.single(background);
    let mut world=TileWorld::new(Grid {cols:1,rows:1,layout:Layout::Orthogonal,wrap_x:false,wrap_y:false,step:(512,384)},sets);
    world.color=color;
    world.free.push(PlacedSprite {screen:true,..PlacedSprite::new(bg,0,0)});
    let mut i=0;
    for em64 in [6*64,9*64+1,12*64,16*64+31,32*64+63] {
        for (bold,italic) in [(false,false),(true,false),(false,true),(true,true)] {
            for (j,ch) in "MWIlÄgjø".chars().enumerate() {
                let x=13+j as i32*49;
                let y=25+i*17;
                let color=match j%4 {0=>[0,0,0],1=>[255,255,255],2=>[17,129,247],_=>[248,21,137]};
                let g=OutlineGlyph {font:0,shape:GlyphShape::Glyph(face.glyph(ch).unwrap()),em64,advance:37,bold,italic,color,key:None};
                let base=PlacedSprite {screen:i%2==0,clip:Some([3,2,507,380]),..PlacedSprite::new(SpriteSetId::EMPTY,x,y)};
                world.free.push(PlacedSprite {text:Some(g),..base});
                world.free.push(PlacedSprite {text:Some(OutlineGlyph {shape:GlyphShape::Bar {top:2,height:1},..g}),..base});
                // The separate keyed draw also uses the shared narrow window.
                if j%4==3 {world.free.push(PlacedSprite {x:x+17,text:Some(OutlineGlyph {key:Some([255,0,255]),..g}),..base});}
            }
            i+=1;
        }
    }
    world.fonts.push(face);
    (world,atlas.finish())
}

#[test]
fn dual_source_and_three_draw_lcd_match_for_real_glyphs_and_partial_alpha() {
    let dual=device(true);
    let fallback=device(false);
    let mut compared=0;
    for model in [ColorModel::Direct,ColorModel::Quantized {bits:4},ColorModel::Quantized {bits:5},ColorModel::Quantized {bits:6},ColorModel::Quantized {bits:7}] {
        let (world,atlas)=world(font(),model);
        let mut a=TileRenderer::new(&dual.device,wgpu::TextureFormat::Rgba8Unorm);
        let mut b=TileRenderer::new(&fallback.device,wgpu::TextureFormat::Rgba8Unorm);
        a.set_atlas(&dual.device,&dual.queue,&atlas); a.set_world(&dual.device,&world);
        b.set_atlas(&fallback.device,&fallback.queue,&atlas); b.set_world(&fallback.device,&world);
        for zoom in [(1,1),(239,256),(3,2),(2,3),(5,4),(2,1)] {
            let camera=Camera {x:-1,y:1,origin:(11,7),screen_shift:(-3,2),zoom,zoomed_art:true,..Camera::new((512,384))};
            // A scaled view also exercises direct intermediate composition.
            for scale in [1,2] {
                let present=Present {canvas:(540,400),target:(540*scale,400*scale),zoom:(scale,1),at:(0,0)};
                let aa=dual.render_presented(&mut a,&[camera],&[],present,0,[19,43,71,83]);
                let bb=fallback.render_presented(&mut b,&[camera],&[],present,0,[19,43,71,83]);
                assert_eq!(aa.pixels,bb.pixels,"model {model:?},zoom {zoom:?},scale {scale}");
                compared+=aa.pixels.len();
            }
        }
    }
    eprintln!("dual/fallback exact RGBA pixel comparisons: {compared}");
}
