//! Original-binary logical key predicates for file/mask sources. These
//! tests never install DLLs into a game or send game pixels to the viewer.
#![cfg(all(windows,target_arch="x86"))]
use std::sync::Arc;
use civ3_ui::{selection::FilePixels,ui::{UiModel,Event,KeySources,FileSample,KEY_RGB}};
use jgl_ffi::layout::Rect32;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress,LoadLibraryA};
unsafe fn call(object: u32, slot: u32, args: &[u32]) -> u32 {
    unsafe {
        let table = *(object as usize as *const u32);
        let function = *((table + slot) as usize as *const u32) as usize;
        match *args {
            [] => std::mem::transmute::<usize, extern "thiscall" fn(u32) -> u32>(function)(object),
            [a] => std::mem::transmute::<usize, extern "thiscall" fn(u32, u32) -> u32>(function)(object, a),
            [a,b] => std::mem::transmute::<usize, extern "thiscall" fn(u32, u32, u32) -> u32>(function)(object, a, b),
            [a,b,c] => std::mem::transmute::<usize, extern "thiscall" fn(u32, u32, u32, u32) -> u32>(function)(object, a, b, c),
            [a,b,c,d] => std::mem::transmute::<usize, extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32>(function)(object, a, b, c, d),
            [a,b,c,d,e] => std::mem::transmute::<usize, extern "thiscall" fn(u32,u32,u32,u32,u32,u32)->u32>(function)(object,a,b,c,d,e),
            [a,b,c,d,e,f] => std::mem::transmute::<usize, extern "thiscall" fn(u32,u32,u32,u32,u32,u32,u32)->u32>(function)(object,a,b,c,d,e,f),
            [a,b,c,d,e,f,g,h] => std::mem::transmute::<usize, extern "thiscall" fn(u32,u32,u32,u32,u32,u32,u32,u32,u32)->u32>(function)(object,a,b,c,d,e,f,g,h),
            _ => panic!("test arity"),
        }
    }
}

fn addr<T>(value: &T) -> u32 { value as *const T as usize as u32 }
fn rect(l: i32, t: i32, r: i32, b: i32) -> Rect32 { Rect32 { left: l, top: t, right: r, bottom: b } }


struct Inputs;
impl KeySources for Inputs {
    fn file(&self,_:&str,_:i32,_:i32,_:Option<&civ3_ui::ui::PaletteInput>)->Option<FileSample> {None}
}

#[test]
fn masks_and_source_versions_match_original_predicates() {
    let path=std::ffi::CString::new(std::env::var("CIV3_JGL_DLL").expect("original oracle required")).unwrap();
    unsafe {
        let module=LoadLibraryA(path.as_ptr().cast());assert!(!module.is_null());
        let export=GetProcAddress(module,c"get_graphsy_object_ptr".as_ptr().cast()).unwrap();
        let g=std::mem::transmute::<unsafe extern "system" fn()->isize,extern "C" fn()->u32>(export)();
        let table=call(g,0x78,&[0]);assert_eq!(call(table,4,&[]),0);
        let palette: [[u8;3];256]=std::array::from_fn(|i|[i as u8,i as u8,i as u8]);
        assert_eq!(call(table,0x14,&[palette.as_ptr() as usize as u32,0,256]),0);
        let a=call(g,0x7C,&[0,0]);let bg=call(g,0x7C,&[0,0]);
        assert_eq!(call(a,4,&[1,1,16,1]),0);assert_eq!(call(bg,4,&[1,1,16,1]),0);
        let mut pairs=0;let mut unknown=0;
        for slot in [0x44,0x50,0x58] {
            for index in [0u8,1,8,63,127,223,253,254,255] {
                for mask in [0u8,1,7,127,248,254,255] {
                    for background in [0u16,0x7c1f,0x7fff,0x1234] {
                        let sprite=call(g,0x80,&[0,0]);let ms=call(g,0x80,&[0,0]);
                        assert_eq!(call(sprite,4,&[addr(&index),1,1,8,0,table]),0);
                        assert_eq!(call(ms,4,&[addr(&mask),1,1,8,0,table]),0);
                        let bits=call(a,0x10,&[]);*(bits as *mut u16)=0x7c1f;call(a,0x24,&[1]);
                        let bits=call(bg,0x10,&[]);*(bits as *mut u16)=background;call(bg,0x24,&[1]);
                        let mut model=UiModel::default();
                        model.apply(Event::Size {image:10,size:(1,1)});
                        model.apply(Event::Fill {canvas:10,rect:[0,0,1,1],rgb:KEY_RGB,alpha:255,clip:None});
                        for (image,spr,value,name) in [(20,30,index,"color"),(21,31,mask,"mask")] {
                            model.apply(Event::Loaded {image,path:name.into(),size:(1,1)});
                            model.loaded_pixels(image,Arc::new(FilePixels {size:(1,1),row:1,indices:vec![value],palette}));
                            model.apply(Event::Sliced {sprite:spr,image,rect:[0,0,1,1]});
                        }
                        let status=match slot {
                            0x44=>{model.apply(Event::Blit {sprite:30,canvas:10,x:0,y:0,clip:None,palette:None});call(sprite,slot,&[a,0,0,table])},
                            0x50=>{model.apply(Event::MaskedBlit {sprite:30,mask:31,background:99,canvas:10,x:0,y:0,clip:None});call(sprite,slot,&[ms,bg,a,0,0,table])},
                            _=>{model.apply(Event::MaskedBlit {sprite:30,mask:31,background:0,canvas:10,x:0,y:0,clip:None});call(sprite,slot,&[ms,a,0,0,table])},
                        };
                        assert_eq!(status,0);
                        let bits=call(a,0x10,&[]);let old=*(bits as *const u16);call(a,0x24,&[1]);
                        match model.keyed_at(10,0,0,&Inputs) {
                            Some(shown)=>{pairs+=1;assert_eq!(shown,old!=0x7c1f,"slot {slot:x}, index {index}, mask {mask}, background {background:x}, pixel {old:x}");},
                            None=>unknown+=1,
                        }
                        call(sprite,0,&[1]);call(ms,0,&[1]);
                    }
                }
            }
        }
        // Compare the immutable source-version contract with original +58,
        // including a rectangle intersected by a smaller current clip.
        let data=call(g,0x7c,&[0,0]);assert_eq!(call(data,4,&[7,5,8,1]),0);
        let row=call(data,0xe0,&[]) as usize;
        let initial=(0..row*5).map(|i|[0,1,253,7,255,254,1][i%7]).collect::<Vec<_>>();
        let ptr=call(data,0x10,&[]);std::slice::from_raw_parts_mut(ptr as *mut u8,initial.len()).copy_from_slice(&initial);call(data,0x24,&[1]);
        let mut model=UiModel::default();
        model.apply(Event::Loaded {image:100,path:"instance.pcx".into(),size:(7,5)});
        model.loaded_pixels(100,Arc::new(FilePixels {size:(7,5),row:row as u32,indices:initial.clone(),palette}));
        model.apply(Event::Sliced {sprite:101,image:100,rect:[0,0,7,5]});
        let clip=rect(2,0,7,4);assert_eq!(call(data,0x34,&[addr(&clip)]),0);
        let requested=rect(-2,1,5,5);
        assert_eq!(call(data,0x58,&[addr(&requested),1,255]),0);
        assert!(model.replace_indices(100,[2,1,5,4],1,255));
        model.apply(Event::Sliced {sprite:102,image:100,rect:[0,0,7,5]});
        let current=call(data,0x10,&[]);
        let mut cells=0;
        for y in 0..5 {for x in 0..7 {
            let civ3_ui::ui::Source::File {retained:Some(state),..}=model.sprite_source(102).unwrap() else {panic!("source not retained")};
            assert_eq!(state.sample(x,y).unwrap().index,*((current as *const u8).add(y as usize*row+x as usize)));cells+=1;
        }}
        call(data,0x24,&[1]);call(data,0,&[1]);
        model.apply(Event::Cleared {image:100});
        model.apply(Event::Copied {sprite:103,source:102});model.apply(Event::Reset {sprite:102});
        for sprite in [101,103] {
            let civ3_ui::ui::Source::File {retained:Some(state),..}=model.sprite_source(sprite).unwrap() else {panic!("retained source lost")};
            for y in 0..5 {for x in 0..7 {
                let before=initial[y as usize*row+x as usize];
                let expected=if sprite==103 && x>=2 && x<5 && y>=1 && y<4 && before==1 {255} else {before};
                assert_eq!(state.sample(x,y).unwrap().index,expected);cells+=1;
            }}
        }
        println!("source versions: {cells} indexed comparisons, including original scoped +58 and pre/post-edit lifetimes, zero differences");
        call(a,0,&[1]);call(bg,0,&[1]);call(table,0,&[1]);call(g,0x0c,&[]);
        println!("file/mask predicates: {pairs} defined comparisons, {unknown} explicitly undecided, zero differences");
    }
}
