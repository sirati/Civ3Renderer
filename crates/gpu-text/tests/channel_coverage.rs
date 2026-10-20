//! The narrow LCD window must preserve the existing seven-subpixel oracle.
use gpu_text::Face;
use tile_render_shared::text::{self, GlyphPlace, TextGlyph, TEXT_BOLD, TEXT_ITALIC};

fn fixture_faces() -> Vec<Face> {
    let line = |a: (f64, f64), b: (f64, f64)| [a, ((a.0+b.0)/2.0, (a.1+b.1)/2.0), b];
    let outer = [(0.0,-120.0),(10.0,720.0),(640.0,700.0),(600.0,-100.0)];
    let hole = [(200.0,100.0),(420.0,90.0),(430.0,500.0),(180.0,490.0)];
    let mut curves: Vec<_> = (0..4).map(|i| line(outer[i],outer[(i+1)%4])).collect();
    curves.extend((0..4).map(|i| line(hole[i],hole[(i+1)%4])));
    curves.push([(600.0,200.0),(1100.0,500.0),(600.0,700.0)]);
    curves.push([(600.0,700.0),(300.0,400.0),(600.0,200.0)]);
    let mut faces = vec![Face::from_curves(1000, &[('M',curves,700)])];
    let install = std::env::var_os("CIV3_INSTALL").map(std::path::PathBuf::from)
        .unwrap_or_else(|| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
    if let Ok(bytes) = std::fs::read(install.join("LSANS.TTF")) {
        faces.push(Face::parse(&bytes, 0).unwrap());
    }
    faces
}

#[test]
fn narrow_lcd_window_matches_seven_subpixel_oracle_for_fractional_styled_glyphs() {
    let mut checked = 0;
    for face in fixture_faces() {
        for ch in "MWIlÄgjø ".chars() {
            let Some(index) = face.glyph(ch) else { continue };
            for em in [6*64,9*64+1,12*64,16*64+32,33*64,256*64] {
                for (n,d) in [(1,1),(239,256),(3,2),(2,3),(5,4),(2,1)] {
                    for flags in [0,TEXT_BOLD,TEXT_ITALIC,TEXT_BOLD|TEXT_ITALIC] {
                        let g=TextGlyph {glyph:face.record(index),face:0,em,advance:em as i32/96+1,flags,..Default::default()};
                        for (dx,dy) in [(0,0),(1,31),(47,73),(95,95)] {
                            let mut p=text::place(&face.words,&g,-2,270,n,d).unwrap();
                            p.x+=dx; p.y+=dy;
                            check(&face.words,&p,&mut checked);
                        }
                    }
                }
            }
        }
    }
    // Decoration rectangles use the same filter, including fractional
    // edges, very thin bars, clipping-like samples outside the quad.
    for width in [1,31,32,95,96,97,293] {
        for height in [1,12,23,96,193] {
            let p=GlyphPlace {x:47,y:-31,scale:0,bold:0,flags:text::TEXT_BAR,glyph:0,face:0,bar_w:width,bar_top:-127,bar_h:height};
            check(&[],&p,&mut checked);
        }
    }
    assert!(checked>15_000,"oracle must exercise enough distinct channel values: {checked}");
    eprintln!("exact narrow-window channel comparisons: {checked}");
}

fn check(words: &[u32],p: &GlyphPlace,checked: &mut usize) {
    let (l,t,r,b)=text::bounds(words,p);
    for (px,py) in [(l-1,t-1),(l,t),(r-1,t),(l,b-1),(r-1,b-1),((l+r)/2,(t+b)/2),((2*l+r)/3,(2*t+b)/3),((l+2*r)/3,(t+2*b)/3),(r,b)] {
        let color=((px as u32).wrapping_mul(0x993571)^((py as u32).wrapping_mul(0x7153A9)))&0xFFFFFF;
        let expected=text::coverage(words,p,px,py,color);
        assert_eq!(text::linear_coverage_all_channels(words,p,px,py),text::linear_coverage(words,p,px,py));
        let actual=std::array::from_fn(|c| text::channel_coverage(words,p,px,py,color,c as u32));
        assert_eq!(actual,expected,"glyph {p:?} pixel {px},{py} color {color:06x}");
        assert_eq!(text::coverage_all_channels(words,p,px,py,color),expected,"shared RGB window: glyph {p:?} pixel {px},{py} color {color:06x}");
        assert_eq!(actual.map(tile_render_shared::alpha_steps),expected.map(tile_render_shared::alpha_steps));
        assert_eq!(text::keyed(actual,color,0xFF00FF),text::keyed(expected,color,0xFF00FF));
        *checked+=3;
    }
}
