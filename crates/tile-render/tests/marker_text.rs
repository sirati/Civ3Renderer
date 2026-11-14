//! Outline text in lightweight overlay markers must reach the GPU in order.
use std::sync::Arc;
use tile_render::{AtlasBuilder, Camera, GlyphShape, Grid, Headless, Layout, OutlineGlyph, PlacedSprite,
    RgbaImage, SolidRect, SpriteSetId, SpriteSets, TileRenderer, TileWorld, gpu_text::Face};

const CLEAR: [u8;4]=[0,0,0,255];
fn solid(x:i32,y:i32,w:u32,h:u32,rgba:[u8;4])->PlacedSprite {
    PlacedSprite {screen:true,view:Some(0),solid:Some(SolidRect {width:w,height:h,rgba}),..PlacedSprite::new(SpriteSetId::EMPTY,x,y)}
}
fn face()->Arc<Face> {
    // A readable H outline. No filesystem font or atlas dependencies.
    let points=[(0.,0.),(0.,800.),(120.,800.),(120.,460.),(380.,460.),(380.,800.),
        (500.,800.),(500.,0.),(380.,0.),(380.,340.),(120.,340.),(120.,0.)];
    let curves=(0..points.len()).map(|i| {
        let (a,b)=(points[i],points[(i+1)%points.len()]);[a,((a.0+b.0)/2.,(a.1+b.1)/2.),b]
    }).collect();
    Arc::new(Face::from_curves(1000,&[('H',curves,600)]))
}
fn text(x:i32,y:i32,color:[u8;3])->PlacedSprite {
    PlacedSprite {screen:true,view:Some(0),text:Some(OutlineGlyph {font:0,shape:GlyphShape::Glyph(0),em64:16*64,
        advance:10,bold:false,italic:false,color,key:None}),..PlacedSprite::new(SpriteSetId::EMPTY,x,y)}
}
fn world(canonical_text:bool)->TileWorld {
    let mut world=TileWorld::new(Grid {cols:1,rows:1,layout:Layout::Orthogonal,step:(1,1),wrap_x:false,wrap_y:false},SpriteSets::default());
    world.fonts=vec![face()];
    world.free=vec![solid(0,28,32,4,[0,0,255,255]),solid(0,0,2,32,[255,255,255,255])];
    if canonical_text { world.free.push(text(20,24,[255,255,255])); }
    world
}
fn expected(world:&TileWorld,markers:&[PlacedSprite],camera:&Camera)->RgbaImage {
    let mut full=world.clone();full.free.extend_from_slice(markers);
    tile_render::reference::render(&full,&AtlasBuilder::new(16).finish(),camera,0,CLEAR)
}
fn near(got:&RgbaImage,want:&RgbaImage) {
    assert_eq!((got.width,got.height),(want.width,want.height));
    let most=got.pixels.iter().zip(&want.pixels).flat_map(|(a,b)|a.iter().zip(b).map(|(a,b)|a.abs_diff(*b))).max().unwrap_or(0);
    assert!(most<=1,"GPU blend rounding differs from reference by {most}, expected at most one 8-bit step");
}
#[test]
fn gpu_marker_text_draws_without_world_text_and_keeps_mixed_painter_order() {
    let gpu=Headless::new().expect("RADV GPU");
    let camera=Camera {cells:false,..Camera::new((32,32))};
    for canonical_text in [false,true] {
        let mut world=world(canonical_text);
        let mut renderer=TileRenderer::new(&gpu.device,wgpu::TextureFormat::Rgba8Unorm, SHADERS);
        renderer.set_world(&gpu.device,&world);
        let markers=vec![solid(3,3,14,22,[120,0,0,255]),
            PlacedSprite {view:Some(0),clip:Some([4,8,16,18]),..text(4,20,[255,160,0])},
            solid(9,8,3,10,[0,255,0,255])];
        renderer.set_markers(&gpu.device,&gpu.queue,&markers);
        let got=gpu.render(&mut renderer,&camera,0,CLEAR);
        near(&got,&expected(&world,&markers,&camera));
        assert_eq!(got.get(10,14),[0,255,0,255],"later solid remains over the glyph");
        assert_eq!(got.get(1,24),[255,255,255,255],"existing line unchanged");
        assert_eq!(got.get(20,30),[0,0,255,255],"existing sprite unchanged");
        assert!(got.pixels.iter().any(|p|p[0]>180 && p[1]>80 && p[1]<220 && p[2]<20),"actual orange H ink");
        let both=gpu.render_views(&mut renderer,&[camera,Camera {origin:(32,0),..camera}],(64,32),0,CLEAR);
        assert_eq!(both.get(40,14),CLEAR,"view-specific marker text cannot leak into another view");
        // Unchanged sprite markers, changed text only, same frame number:
        // cached-picture identity must include text marker commands too.
        let present=tile_render::Present {canvas:(32,32),zoom:(2,1),at:(0,0),target:(64,64)};
        let before=gpu.render_presented(&mut renderer,&[camera],&[],present,0,CLEAR);
        let mut changed=markers.clone();changed[1].text.as_mut().unwrap().color=[0,255,255];
        renderer.set_markers(&gpu.device,&gpu.queue,&changed);
        let after=gpu.render_presented(&mut renderer,&[camera],&[],present,0,CLEAR);
        assert_ne!(before,after,"cached scaled picture must observe a text-only marker change");
        let mut fresh=TileRenderer::new(&gpu.device,wgpu::TextureFormat::Rgba8Unorm, SHADERS);
        fresh.set_world(&gpu.device,&world);fresh.set_markers(&gpu.device,&gpu.queue,&changed);
        assert_eq!(after,gpu.render_presented(&mut fresh,&[camera],&[],present,0,CLEAR));
        near(&gpu.render(&mut renderer,&camera,0,CLEAR),&expected(&world,&changed,&camera));
        // Cell/free/full updates retain marker text rather than changing its
        // text-buffer offset into canonical world glyph records.
        world.free.push(solid(24,0,3,3,[255,0,255,255]));
        renderer.update_free(&gpu.device,&gpu.queue,&world);
        near(&gpu.render(&mut renderer,&camera,0,CLEAR),&expected(&world,&changed,&camera));
        renderer.set_world(&gpu.device,&world);
        near(&gpu.render(&mut renderer,&camera,0,CLEAR),&expected(&world,&changed,&camera));
        renderer.set_markers(&gpu.device,&gpu.queue,&[]);
        near(&gpu.render(&mut renderer,&camera,0,CLEAR),&expected(&world,&[],&camera));
    }
}

/// The tile renderer's independent GPU-test shader module.
const SHADERS: tile_render::Shaders = tile_render::Shaders::new(tile_render_test_spirv::SPIRV, tile_render_test_spirv::SPIRV_FALLBACK);
