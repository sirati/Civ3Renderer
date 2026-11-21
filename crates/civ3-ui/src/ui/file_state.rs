//! Immutable decoded file inputs with ordered, rectangle-scoped index
//! replacements. Sprites and display commands retain the version they used.

use std::{cmp::Ordering,hash::{Hash,Hasher},sync::{Arc,atomic::{AtomicU64,Ordering as AtomicOrdering}}};
use crate::selection::FilePixels;

#[derive(Clone)]
pub struct FileState(Arc<State>);

/// A pixel of a loaded file: its palette index and colour.
#[derive(Clone, Copy, Debug)]
pub struct FileSample {
    pub index: u8,
    pub rgb: [u8; 3],
}
struct State {
    id:u64,
    pixels:Arc<FilePixels>,
    changes:Vec<menu::screen::IndexChange>,
    /// Indices present in the pixels, after the replacements.
    used:[bool;256],
}

impl FileState {
    pub fn new(pixels:Arc<FilePixels>)->Self {
        let mut used=[false;256];
        for &index in &pixels.indices {used[index as usize]=true;}
        Self::version(pixels,Vec::new(),used)
    }
    fn version(pixels:Arc<FilePixels>,changes:Vec<menu::screen::IndexChange>,used:[bool;256])->Self {
        static NEXT:AtomicU64=AtomicU64::new(1);
        Self(Arc::new(State {id:NEXT.fetch_add(1,AtomicOrdering::Relaxed),pixels,changes,used}))
    }
    pub fn replaced(&self,change:menu::screen::IndexChange)->Self {
        // A superset: the replaced index may survive outside the rectangle.
        let mut used=self.0.used;
        if used[change.from as usize] {used[change.to as usize]=true;}
        let mut changes=self.0.changes.clone();changes.push(change);
        Self::version(self.0.pixels.clone(),changes,used)
    }
    pub fn changes(&self)->&[menu::screen::IndexChange] {&self.0.changes}
    /// Whether `index` may be among the pixels (after the replacements).
    pub fn uses(&self,index:u8)->bool {self.0.used[index as usize]}
    pub fn palette_color(&self,index:u8)->[u8;3] {self.0.pixels.palette[index as usize]}
    /// Whether a drawn index may be `rgb` in RGB555. `keyed`: 254 and 255
    /// draw nothing.
    pub fn may_draw(&self,rgb:[u8;3],keyed:bool)->bool {
        (0..if keyed {254} else {256}).any(|i|self.0.used[i] && self.0.pixels.palette[i].map(|c|c>>3)==rgb.map(|c|c>>3))
    }
    /// The pixels of `rect` (x, y, w, h) drawn in `rgb` (RGB555), as
    /// rectangles: row runs, merged down while the next row repeats them.
    /// None past `limit` rectangles.
    pub fn runs(&self,rgb:[u8;3],keyed:bool,[x,y,w,h]:[i32;4],limit:usize)->Option<Vec<[i32;4]>> {
        if !self.may_draw(rgb,keyed) {return Some(Vec::new());}
        let drawn=|sx,sy|self.sample(sx,sy).is_some_and(|s|(!keyed || s.index<254) && s.rgb.map(|c|c>>3)==rgb.map(|c|c>>3));
        let mut done=Vec::new();
        let mut open:Vec<[i32;4]>=Vec::new();
        for sy in y..y+h {
            let mut row=Vec::new();
            let mut sx=x;
            while sx<x+w {
                if !drawn(sx,sy) {sx+=1;continue;}
                let start=sx;
                while sx<x+w && drawn(sx,sy) {sx+=1;}
                row.push((start,sx));
            }
            let mut next=Vec::new();
            for (l,r) in row {
                match open.iter().position(|o|o[0]==l && o[0]+o[2]==r) {
                    Some(i)=>{let mut o=open.swap_remove(i);o[3]+=1;next.push(o);}
                    None=>next.push([l,sy,r-l,1]),
                }
            }
            done.append(&mut open);
            open=next;
            if done.len()+open.len()>limit {return None;}
        }
        done.append(&mut open);
        Some(done)
    }
    pub fn sample(&self,x:i32,y:i32)->Option<FileSample> {
        let mut index=self.0.pixels.index(x,y)?;
        for change in &self.0.changes {
            let [l,t,r,b]=change.rect;
            if x>=l && y>=t && x<r && y<b && index==change.from {index=change.to;}
        }
        Some(FileSample {index,rgb:self.0.pixels.palette[index as usize]})
    }
}
impl std::fmt::Debug for FileState {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
        f.debug_struct("FileState").field("id",&self.0.id).field("size",&self.0.pixels.size).field("changes",&self.0.changes).finish()
    }
}
impl PartialEq for FileState {fn eq(&self,other:&Self)->bool {self.0.id==other.0.id}}
impl Eq for FileState {}
impl PartialOrd for FileState {fn partial_cmp(&self,other:&Self)->Option<Ordering>{Some(self.cmp(other))}}
impl Ord for FileState {fn cmp(&self,other:&Self)->Ordering {self.0.id.cmp(&other.0.id)}}
impl Hash for FileState {fn hash<H:Hasher>(&self,h:&mut H){self.0.id.hash(h)}}
