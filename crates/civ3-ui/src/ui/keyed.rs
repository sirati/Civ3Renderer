//! Logical colour-key queries over retained commands. This never allocates
//! or draws a canvas. Intervals preserve uncertainty about glyph coverage;
//! later opaque commands replace it only at the point they actually cover.

use super::*;

#[derive(Clone, Copy, Debug)]
pub struct FileSample {
    pub index: u8,
    pub rgb: [u8; 3],
}

pub trait KeySources {
    fn file(&self, path: &str, x: i32, y: i32, palette: Option<&menu::screen::PaletteFrom>) -> Option<FileSample>;
    /// The renderer's exact keyed text pixel at (x, y). `Some(None)` is the
    /// colour key; `None` means a retained renderer source is unavailable.
    fn text_keyed(&self, _run:&TextRun, _x:i32, _y:i32)->Option<Option<[u8;3]>> { None }
    fn palette(&self,_palette:&menu::screen::PaletteFrom,_index:u8)->Option<[u8;3]> {None}
    fn surface(&self,_image:u32,_x:i32,_y:i32)->Option<[u8;3]> {None}
    fn flc(&self,_path:&str,_frame:u32,_x:i32,_y:i32,_plain:bool)->Option<Option<FileSample>> {None}
}

/// Inclusive channel ranges, in RGB555's five-bit channels. A missing
/// glyph source can enlarge a range but cannot turn it into a known hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Colors { lo: [u8; 3], hi: [u8; 3] }

impl Colors {
    const UNKNOWN: Self = Self { lo: [0; 3], hi: [31; 3] };
    const KEY: Self = Self { lo: [31, 0, 31], hi: [31, 0, 31] };
    fn rgb(rgb: [u8; 3]) -> Self { let c=rgb.map(|c| c >> 3); Self { lo: c, hi: c } }
    fn union(self, other: Self) -> Self {
        Self { lo: std::array::from_fn(|i| self.lo[i].min(other.lo[i])), hi: std::array::from_fn(|i| self.hi[i].max(other.hi[i])) }
    }
    fn keyed(self) -> Option<bool> {
        if self == Self::KEY { return Some(false); }
        ([31,0,31].into_iter().enumerate().any(|(i,c)| c < self.lo[i] || c > self.hi[i])).then_some(true)
    }
    fn masked(self, rgb: [u8; 3], mask: u8) -> Self {
        // jgl +58: mask FF leaves the destination; mask 0 first writes
        // the source. Other weights use mask+1 destination /256 and
        // 255-mask source /256, truncating each term separately.
        if mask == 255 { return self; }
        let under = if mask == 0 { Self::rgb(rgb) } else { self };
        let mix = |c: [u8; 3]| std::array::from_fn(|i| {
            (((u32::from(rgb[i]) * (255-u32::from(mask)) >> 8)
                + ((u32::from(c[i]) << 3) * (u32::from(mask)+1) >> 8)) >> 3) as u8
        });
        Self { lo: mix(under.lo), hi: mix(under.hi) }
    }
    fn premultiplied(rgb:[u8;3],mask:u8)->Self {
        if mask==0 {return Self::rgb(rgb);}
        // +50 ORs the premultiplied palette word with the weighted
        // background channels. Those source bits survive every background.
        let lo=rgb.map(|c|c>>3);
        if lo[1]!=0 {return Self {lo,hi:[31;3]};}
        let hi=std::array::from_fn(|i| if i==1 {31} else {
            (0..32u32).map(|bg|(u32::from(lo[i]) | (u32::from(lo[i])+(bg*u32::from(mask)>>8))) as u8 &31).max().unwrap()
        });
        Self {lo,hi}
    }
    fn shade(self,level:u8)->Self {
        let scale=|c:[u8;3]|c.map(|c|(u32::from(c)*u32::from(level)/16) as u8);
        Self {lo:scale(self.lo),hi:scale(self.hi)}
    }
}

fn sample(files:&impl KeySources,path:&str,state:Option<&FileState>,x:i32,y:i32,palette:Option<&menu::screen::PaletteFrom>)->Option<FileSample> {
    let Some(state)=state else {return files.file(path,x,y,palette)};
    let mut s=state.sample(x,y)?;
    if let Some(p)=palette.filter(|p|u32::from(s.index)>=p.first && u32::from(s.index)<p.first+p.count) {s.rgb=files.palette(p,s.index)?;}
    Some(s)
}

/// The resolved records that may draw at (x, y), oldest first.
pub(super) fn records_at(model:&UiModel,canvas:u32,x:i32,y:i32)->String {
    let size=model.sizes.get(&canvas).copied().unwrap_or((i32::MAX,i32::MAX));
    model.canvases.get(&canvas).into_iter().flat_map(|ops|ops.iter()).flat_map(|op|model.resolve(op,0))
        .filter(|op|{let [l,t,r,b]=visible_bounds(op,Some(size));x>=l && y>=t && x<r && y<b})
        .map(|op|format!("{op:?}")).collect::<Vec<_>>().join(" | ")
}

pub(super) fn query(model: &UiModel, canvas: u32, x: i32, y: i32, files: &impl KeySources) -> Option<bool> {
    query_coverage(model, canvas, x, y, files, false)
}

/// Vary only Text records' coverage. Later opaque non-text records still
/// replace the interval, so a fill/file/tile error cannot become text merely
/// because the canvas also contains labels.
pub(super) fn text_raster_at(model: &UiModel, canvas: u32, x: i32, y: i32, files: &impl KeySources) -> bool {
    query(model,canvas,x,y,files).is_some() && query_coverage(model,canvas,x,y,files,true).is_none()
}

pub(super) fn unwritten_at(model: &UiModel, canvas: u32, x: i32, y: i32) -> bool {
    let Some(&size)=model.sizes.get(&canvas) else {return false};
    !model.unanswered.contains(&canvas) && !model.files.contains_key(&canvas) && !model.generated.contains(&canvas) && !model.keyed_clear.contains(&canvas) &&
        !model.canvases.get(&canvas).into_iter().flat_map(|ops|ops.iter()).flat_map(|op|model.resolve(op,0))
            .any(|op| {let [l,t,r,b]=visible_bounds(&op,Some(size)); x>=l && y>=t && x<r && y<b})
}

fn query_coverage(model: &UiModel, canvas: u32, x: i32, y: i32, files: &impl KeySources, vary_text: bool) -> Option<bool> {
    let Some(ops)=model.canvases.get(&canvas) else {
        // Files and generated surfaces have an independent source; absence
        // of canvas commands does not make those sources transparent.
        if model.unanswered.contains(&canvas) || model.files.contains_key(&canvas) || model.generated.contains(&canvas) {return None;}
        return model.sizes.contains_key(&canvas).then_some(false);
    };
    let size=model.sizes.get(&canvas)?;
    // Retained canvases start transparent in the viewer. Unwritten legacy
    // storage (including padded corners) contributes no draw command.
    let mut colors = Colors::KEY;
    if model.unanswered.contains(&canvas) { colors = Colors::UNKNOWN; }
    for op in ops.iter().flat_map(|op|model.resolve(op,0)) {
        if let Op::KeyClear {rect:[l,t,w,h]}=op {
            if x>=l && y>=t && x<l+w && y<t+h { colors=Colors::KEY; }
            continue;
        }
        if let Op::Unknown {rect:[l,t,w,h]}=op {
            if x>=l && y>=t && x<l+w && y<t+h { colors=Colors::UNKNOWN; }
            continue;
        }
        let Some(draw)=super::screen_draw(op,(0,0),[0,0,size.0,size.1]) else {continue};
        let draw=&draw;
        let contains = |[l,t,r,b]: [i32;4]| x>=l && y>=t && x<r && y<b;
        match draw {
            ScreenDraw::Fill { rect:[l,t,w,h], rgb, alpha, clip } if contains([*l,*t,l+w,t+h]) && contains(*clip) => {
                colors=if *alpha==255 { Colors::rgb(*rgb) } else { colors.union(Colors::rgb(*rgb)) };
            }
            ScreenDraw::Text { run, clip } if contains(*clip) => {
                if vary_text {
                    let bounds=visible_bounds(&Op::Text(run.clone(),*clip),Some(*size));
                    if contains(bounds) { colors=colors.union(Colors::rgb(run.color)); }
                    continue;
                }
                if colors == Colors::KEY {
                    match files.text_keyed(run, x, y) {
                        Some(None) => {}
                        Some(Some(rgb)) => colors = Colors::rgb(rgb),
                        None => colors = colors.union(Colors::rgb(run.color)),
                    }
                } else if !matches!(files.text_keyed(run, x, y), Some(None)) {
                    colors = colors.union(Colors::rgb(run.color));
                }
            }
            ScreenDraw::Image { retained,path, rect, x:dx, y:dy, keyed, clip, mask, palette, opacity }
                if contains([*dx,*dy,dx+rect[2],dy+rect[3]]) && contains(*clip) => {
                let at=(rect[0]+x-dx,rect[1]+y-dy);
                colors=match sample(files,path,retained.as_ref(),at.0,at.1,palette.as_ref()) {
                    None=>Colors::UNKNOWN,
                    Some(s) if *keyed && s.index>=if mask.is_some() {255} else {254}=>colors,
                    Some(s)=>{
                        let next=if let Some(mask)=mask {
                            match sample(files,&mask.path,mask.retained.as_ref(),mask.at.0+x-dx,mask.at.1+y-dy,None) {
                                Some(m) if m.index==255=>colors,
                                Some(m) if mask.premultiplied=>Colors::premultiplied(s.rgb,m.index),
                                Some(m)=>colors.masked(s.rgb,m.index), None=>Colors::UNKNOWN,
                            }
                        } else { Colors::rgb(s.rgb) };
                        if *opacity==255 { next } else { colors.union(next) }
                    }
                };
            }
            ScreenDraw::ResizedImage { x:dx,y:dy,size,clip,.. } if contains([*dx,*dy,dx+size.0,dy+size.1]) && contains(*clip) => colors=Colors::UNKNOWN,
            ScreenDraw::Indexed { retained,path,rect,x:dx,y:dy,clip,palette,indices } if contains(*clip) => {
                let (w,h)=indices.size(*rect);
                if !contains([*dx,*dy,dx+w,dy+h]) {continue;}
                let step=indices.resize.map_or([65536;2],|r|r.sample_step);
                let sx=rect[0]+((i64::from(x-dx)*i64::from(step[0])+i64::from(indices.sample_offset[0]))>>16) as i32;
                let sy=rect[1]+((i64::from(y-dy)*i64::from(step[1])+i64::from(indices.sample_offset[1]))>>16) as i32;
                let s=sample(files,path,retained.as_ref(),sx,sy,palette.as_ref());
                colors=match s {
                    None=>Colors::UNKNOWN,
                    Some(s) if !indices.shade.is_empty()=>indices.shade.iter().find(|(i,_)|*i==s.index).map_or(colors,|(_,k)|colors.union(colors.shade(*k))),
                    Some(s) if indices.palette_alpha.is_some() && (224..240).contains(&s.index)=>Colors::UNKNOWN,
                    Some(s) if indices.transparent.contains(&s.index)=>colors,
                    Some(s)=>Colors::rgb(s.rgb),
                };
            }
            ScreenDraw::Surface { image,rect,x:dx,y:dy,clip,.. } if contains([*dx,*dy,dx+rect[2],dy+rect[3]]) && contains(*clip) => colors=files.surface(*image,rect[0]+x-dx,rect[1]+y-dy).map_or(Colors::UNKNOWN,Colors::rgb),
            ScreenDraw::Flc { path,chunk,x:dx,y:dy,clip,plain } if contains(*clip) => {
                colors=match files.flc(path,*chunk,x-dx,y-dy,*plain) {
                    Some(None)=>colors,
                    Some(Some(s)) if (*plain && s.index>=254) || s.index==255=>colors,
                    Some(Some(s)) if !*plain && s.index>=240=>colors.union(colors.shade(s.index-239)),
                    Some(Some(s)) if !*plain && s.index>=224=>Colors::UNKNOWN,
                    Some(Some(s))=>Colors::rgb(s.rgb),
                    None if x<*dx || y<*dy=>colors,
                    None=>Colors::UNKNOWN,
                };
            }
            _=>{}
        }
    }
    colors.keyed()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Retained;
    impl KeySources for Retained {
        fn file(&self,_:&str,_:i32,_:i32,_:Option<&menu::screen::PaletteFrom>)->Option<FileSample> {None}
    }

    fn sources(m:&mut UiModel,palette:[[u8;3];256]) {
        // 8x4 file: columns 0..4 index 1, columns 4..8 index 0xFF.
        let indices=(0..32).map(|i|if i%8<4 {1} else {0xFF}).collect();
        m.apply(Event::Loaded {image:10,path:"oldinterface".into(),size:(8,4)});
        m.loaded_pixels(10,Arc::new(crate::selection::FilePixels {size:(8,4),row:8,indices,palette}));
        m.apply(Event::Sliced {sprite:20,image:10,rect:[0,0,8,4]});
        m.apply(Event::Size {image:30,size:(8,4)});
        m.apply(Event::Blit {sprite:20,canvas:30,x:0,y:0,clip:None,palette:None});
        m.apply(Event::Reset {sprite:20});
        m.apply(Event::Cleared {image:10});
        // A frame piece: index 2 in a 2x2 file, the rest keyed.
        m.apply(Event::Loaded {image:11,path:"piece".into(),size:(2,2)});
        m.loaded_pixels(11,Arc::new(crate::selection::FilePixels {size:(2,2),row:2,indices:vec![2,0xFF,0xFF,0xFF],palette}));
        m.apply(Event::Sliced {sprite:21,image:11,rect:[0,0,2,2]});
    }

    #[test]
    fn empty_retained_canvas_is_transparent_but_missing_external_sources_are_unknown() {
        let mut m=UiModel::new(vec![],vec![]);
        m.apply(Event::Size {image:1,size:(15,15)});
        assert_eq!(m.keyed_at(1,14,14,&Retained),Some(false));
        assert!(m.unwritten_at(1,14,14));
        m.apply(Event::Loaded {image:2,path:"missing.pcx".into(),size:(15,15)});
        assert_eq!(m.keyed_at(2,14,14,&Retained),None);
        assert!(!m.unwritten_at(2,14,14));
        m.add_generated(1);
        assert_eq!(m.keyed_at(1,14,14,&Retained),None);
        assert!(!m.unwritten_at(1,14,14));
    }

    #[test]
    fn mill_hits_keep_both_sampling_phases_after_clipped_copy_and_source_release() {
        // The 33-to-24 renderer samples, including C3X's different x/y
        // phases. Alternating ink/key source pixels expose a phase reset.
        const X: [usize; 24] = [0,2,3,5,6,7,9,10,11,13,14,16,17,18,20,21,22,24,25,27,28,29,31,32];
        const Y: [usize; 24] = [0,1,3,4,6,7,8,10,11,12,14,15,17,18,19,21,22,23,25,26,28,29,30,32];
        let ink = |x: usize, y: usize| (x * 3 + y * 5) % 7 < 3;
        let mut palette = [[0; 3]; 256];
        palette[7] = KEY_RGB;
        let mut m = UiModel::new(vec![], vec![]);
        m.apply(Event::Loaded { image: 10, path: "Art/resources.pcx".into(), size: (50, 50) });
        m.loaded_pixels(10, Arc::new(crate::selection::FilePixels {
            size: (50, 50), row: 50, palette,
            indices: (0..2500).map(|i| if ink(i % 50, i / 50) { 1 } else { 7 }).collect(),
        }));
        m.apply(Event::Size { image: 30, size: (80, 80) });
        m.apply(Event::MillIcons { canvas: 30, path: "Art/resources.pcx".into(),
            resources: vec![(0, 0)], at: (5, 7), clip: [22, 9, 42, 28], transparent: vec![7] });
        m.apply(Event::Size { image: 40, size: (160, 160) });
        m.apply(Event::Copy { from: 30, src: [21, 8, 23, 23], canvas: 40, x: 100, y: 100, keyed: true });
        m.apply(Event::Cleared { image: 10 });
        // Queries must retain the old sheet, even if its address is reused.
        m.apply(Event::Loaded { image: 10, path: "other.pcx".into(), size: (50, 50) });
        m.loaded_pixels(10, Arc::new(crate::selection::FilePixels {
            size: (50, 50), row: 50, palette, indices: vec![7; 2500],
        }));
        let mut hits = 0;
        let mut holes = 0;
        for dy in 0..24 {
            for dx in 0..24 {
                let visible = dx >= 2 && dx < 22 && dy >= 2 && dy < 21;
                let expected = visible && ink(9 + X[dx], 9 + Y[dy]);
                assert_eq!(m.keyed_at(30, 20 + dx as i32, 7 + dy as i32, &Retained), Some(expected), "icon ({dx},{dy})");
                assert_eq!(m.keyed_at(40, 99 + dx as i32, 99 + dy as i32, &Retained), Some(expected), "copied icon ({dx},{dy})");
                hits += usize::from(expected);
                holes += usize::from(visible && !expected);
            }
        }
        assert!(hits > 0 && holes > 0);
    }

    #[test]
    fn text_raster_diagnostic_follows_record_types_and_later_occlusion() {
        struct Text;
        impl KeySources for Text {
            fn file(&self,_:&str,_:i32,_:i32,_:Option<&menu::screen::PaletteFrom>)->Option<FileSample> {None}
            fn text_keyed(&self,_:&TextRun,_:i32,_:i32)->Option<Option<[u8;3]>> {Some(None)}
        }
        let mut m=UiModel::new(vec![],vec![]);
        m.apply(Event::Size {image:1,size:(30,30)});
        let run=TextRun {x:2,y:15,color:[0;3],face:"fixture".into(),height:-12,weight:400,italic:false,underline:false,text:"A".into(),backdrop:None};
        m.apply(Event::TextClipped {canvas:1,run,clip:[0,0,30,30]});
        assert_eq!(m.keyed_at(1,4,10,&Text),Some(false));
        assert!(m.text_raster_at(1,4,10,&Text));
        assert!(!m.text_raster_at(1,25,25,&Text),"outside the run, even inside its clip");
        m.apply(Event::Fill {canvas:1,rect:[3,9,3,3],rgb:[0;3],alpha:255,clip:None});
        assert_eq!(m.keyed_at(1,4,10,&Text),Some(true));
        assert!(!m.text_raster_at(1,4,10,&Text),"a later fill is a fill mismatch");
        m.unanswered_in(1,[3,9,3,3]);
        assert_eq!(m.keyed_at(1,4,10,&Text),None);
        assert!(!m.text_raster_at(1,4,10,&Text),"an unsupported source remains unknown");
    }

    /// The exe's frame textures: a fresh 8-bit image holding one file
    /// region (0x549E03), tiled by +38 where the frame canvas shows the
    /// table's colour at the tile's key (0x5FCD40), after the file image,
    /// the sprite and its slice are gone.
    #[test]
    fn tiles_from_composed_images_answer_through_their_file_region_or_refuse() {
        let mut palette=[[0u8;3];256];
        palette[1]=[200,100,50];
        palette[2]=[8,8,8];
        palette[0xFD]=[96,64,32];
        let mut m=UiModel::new(vec![],vec![]);
        sources(&mut m,palette);
        m.apply(Event::Size {image:40,size:(8,8)});
        assert_eq!(m.tile_under(30,[0,0,4,4],40,[0,0,4,4],0xFD,Some(&palette)),Ok(()),"unwritten viewer pixels are transparent");
        m.apply(Event::Fill {canvas:40,rect:[0,0,8,8],rgb:KEY_RGB,alpha:255,clip:None});
        assert_eq!(m.tile_under(30,[0,0,4,4],40,[0,0,4,4],0xFD,Some(&palette)),Ok(()),"no pixel of the tiled colour: unchanged");
        assert_eq!(m.keyed_at(40,1,1,&Retained),Some(false));
        m.apply(Event::Fill {canvas:40,rect:[0,0,8,6],rgb:[96,64,32],alpha:255,clip:None});
        m.apply(Event::Blit {sprite:21,canvas:40,x:0,y:0,clip:None,palette:None});
        assert_eq!(m.tile_under(30,[0,0,4,4],40,[0,0,4,4],0xFD,Some(&palette)),Ok(()));
        // Under the tile: the texture, not the key; the piece stays on top.
        assert_eq!(m.keyed_at(40,1,1,&Retained),Some(true));
        assert_eq!(m.keyed_at(40,0,0,&Retained),Some(true));
        assert_eq!(m.keyed_at(40,1,6,&Retained),Some(false),"below the fill: the key");
        // This source region itself contains transparent pixels and cannot
        // provide an opaque file tile, regardless of the target's base.
        assert_eq!(m.tile_under(30,[2,0,4,4],40,[4,0,4,4],0xFD,Some(&palette)),Err("source's top record is transparent inside the tile"));
        let mut other=palette;other[1]=[0,0,0];
        assert!(m.tile_under(30,[0,0,4,4],40,[0,0,4,4],0xFD,Some(&other)).is_err());
        // A piece's own pixels of the tiled colour show the tile (here a
        // key-coloured texture, so the hit test sees through them).
        let mut same=palette;same[2]=[96,64,32];same[1]=KEY_RGB;
        let mut n=UiModel::new(vec![],vec![]);
        sources(&mut n,same);
        n.apply(Event::Size {image:40,size:(8,8)});
        n.apply(Event::Fill {canvas:40,rect:[0,0,8,8],rgb:[1,2,3],alpha:255,clip:None});
        n.apply(Event::Blit {sprite:21,canvas:40,x:0,y:0,clip:None,palette:None});
        assert_eq!(n.keyed_at(40,0,0,&Retained),Some(true));
        assert_eq!(n.tile_under(30,[0,0,4,4],40,[0,0,4,4],0xFD,Some(&same)),Ok(()));
        assert_eq!(n.keyed_at(40,0,0,&Retained),Some(false),"the piece's pixel shows the tile");
        assert_eq!(n.keyed_at(40,1,0,&Retained),Some(true),"the fill around it is another colour");
        m.unanswered_in(30,[1,1,1,1]);
        assert!(m.tile_under(30,[0,0,4,4],40,[0,0,4,4],0xFD,Some(&palette)).is_err());
        assert!(m.tile_under(30,[2,2,2,2],40,[2,2,2,2],0xFD,Some(&palette)).is_ok(),"the unknown pixel is outside this window");
    }

    /// A magenta key: the canvas's whole-canvas key clear is the base.
    #[test]
    fn a_key_coloured_tile_replaces_the_key_cleared_base_under_pieces() {
        let mut palette=[[0u8;3];256];
        palette[1]=[200,100,50];
        palette[2]=[8,8,8];
        palette[0xFD]=KEY_RGB;
        let mut m=UiModel::new(vec![],vec![]);
        sources(&mut m,palette);
        m.apply(Event::Size {image:40,size:(8,8)});
        m.apply(Event::Fill {canvas:40,rect:[0,0,8,8],rgb:KEY_RGB,alpha:255,clip:None});
        m.apply(Event::Blit {sprite:21,canvas:40,x:0,y:0,clip:None,palette:None});
        assert_eq!(m.keyed_at(40,1,1,&Retained),Some(false));
        assert_eq!(m.tile_under(30,[0,0,4,4],40,[0,0,4,4],0xFD,Some(&palette)),Ok(()));
        assert_eq!(m.keyed_at(40,1,1,&Retained),Some(true));
        assert_eq!(m.keyed_at(40,5,1,&Retained),Some(false),"outside the tile");
    }

    #[test]
    fn unknown_glyph_coverage_only_proves_colour_invariants() {
        let paper=Colors::rgb([210,190,150]);
        assert_eq!(paper.union(Colors::rgb([0;3])).keyed(),Some(true));
        assert_eq!(paper.union(Colors::rgb([255;3])).keyed(),Some(true));
        assert_eq!(Colors::KEY.union(Colors::rgb([0;3])).keyed(),None);
        assert_eq!(Colors::UNKNOWN.union(paper).keyed(),None);
        assert_eq!(Colors::KEY.keyed(),Some(false));
    }
}
