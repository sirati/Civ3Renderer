//! Immutable animation inputs shared by asset loading and logical readers.
//! Queries only look up decoded sources; they never read or decode files.

use std::{collections::HashMap,sync::Arc};
use crate::{flc,Palette};

static ANIMATIONS:handoff::Latest<HashMap<String,Arc<flc::Flc>>>=handoff::Latest::new();
type EncodedAnimations=HashMap<String,(Arc<[u8]>,Arc<flc::Flc>)>;
static INPUTS:handoff::Latest<EncodedAnimations>=handoff::Latest::new();
static PALETTES:handoff::Latest<HashMap<String,Palette>>=handoff::Latest::new();
fn key(path:&str)->String {path.replace('\\',"/").to_ascii_lowercase()}

pub fn animation(source:&str,identity:&str,bytes:&[u8])->Result<Arc<flc::Flc>,flc::FlcError> {
    let identity=key(identity);
    let anim=INPUTS.load().and_then(|m|m.get(&identity).filter(|(input,_)|input.as_ref()==bytes).map(|(_,a)|a.clone())).map_or_else(||flc::decode(bytes).map(Arc::new),Ok)?;
    INPUTS.update(|old|{let mut m=old.cloned().unwrap_or_default();m.insert(identity.clone(),(Arc::from(bytes),anim.clone()));Some(m)});
    ANIMATIONS.update(|old| {
        let mut m=old.cloned().unwrap_or_default();
        m.insert(identity.clone(),anim.clone());m.insert(key(source),anim.clone());Some(m)
    });
    Ok(anim)
}
pub fn palette(source:&str,palette:Palette) {
    PALETTES.update(|old|{let mut m=old.cloned().unwrap_or_default();m.insert(key(source),palette.clone());Some(m)});
}

/// Outer None means unavailable, inner None lies outside the source.
pub fn sample(source:&str,frame:u32,x:i32,y:i32,recolor:Option<&str>)->Option<Option<(u8,[u8;3])>> {
    let animations=ANIMATIONS.load()?;
    let a=animations.get(&key(source))?;
    let f=a.frames.get(frame as usize)?;
    if x<0 || y<0 || x as u32>=a.width || y as u32>=a.height {return Some(None);}
    let index=f.pixels[y as usize*a.width as usize+x as usize];
    // As the viewer's unit art (`remap = [0, 64]`): the civ's first 64 colours.
    let crate::Rgb(r,g,b)=if let Some(recolor)=recolor.filter(|_|index<64) {
        PALETTES.load()?.get(&key(recolor))?.get(index)
    } else {f.palette.get(index)};
    Some(Some((index,[r,g,b])))
}
