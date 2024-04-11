/*use std::borrow::{Borrow, BorrowMut};
use std::ops::{Deref, DerefMut};*/
use std::marker::PhantomData;
use crate::gameview::sealed::{NonTiledMarker, StaticMarker, TiledMarker};
use crate::gameview::traits::{Animated, AnimatedMut, NonTiled, Sprite, Static, Tiled, TiledMut};

mod sealed {
    pub use sealed_intern::{AnimatedMarker, TiledMarker, IsTiledMarker, IsStaticMarker, StaticMarker, NonTiledMarker};
    mod sealed_intern {
        use std::marker::PhantomData;
        use super::super::traits::{Animated, NonTiled, Sprite, Static, Tiled};
    
        pub trait Sealed {
            type TSprite: Sprite;
        }
        //<editor-fold desc="IsStaticMarker">
        pub trait IsStaticMarker : Sealed {}
    
        pub struct StaticMarker<TSprite: Sprite<Static=StaticMarker<TSprite>> + Static>(PhantomData<TSprite>);
    
    
        pub struct AnimatedMarker<TSprite: Sprite<Static=AnimatedMarker<TSprite>> + Animated>(PhantomData<TSprite>);
    
        impl<TSprite> Sealed for StaticMarker<TSprite>
            where TSprite: Sprite<Static=StaticMarker<TSprite>> + Static
        { type TSprite = TSprite; }
    
        impl<TSprite> IsStaticMarker for StaticMarker<TSprite>
            where TSprite: Sprite<Static=StaticMarker<TSprite>> + Static
        {}
    
        impl<TSprite> Sealed for AnimatedMarker<TSprite>
            where TSprite: Sprite<Static=AnimatedMarker<TSprite>> + Animated
        { type TSprite = TSprite; }
    
        impl<TSprite> IsStaticMarker for AnimatedMarker<TSprite>
            where TSprite: Sprite<Static=AnimatedMarker<TSprite>> + Animated
        {}
        //</editor-fold>
        //<editor-fold desc="IsTiledMarker">
        pub trait IsTiledMarker : Sealed{}
    
        pub struct TiledMarker<TSprite: Sprite<Tiled=TiledMarker<TSprite, WIDTH, HEIGHT>> + Tiled<WIDTH, HEIGHT>, const WIDTH: u8, const HEIGHT: u8>(PhantomData<TSprite>);
    
        pub struct NonTiledMarker<TSprite: Sprite<Tiled=NonTiledMarker<TSprite>> + NonTiled>(PhantomData<TSprite>);
    
        impl<TSprite, const WIDTH: u8, const HEIGHT: u8> Sealed for TiledMarker<TSprite, WIDTH, HEIGHT>
            where TSprite: Sprite<Tiled=TiledMarker<TSprite, WIDTH, HEIGHT>> + Tiled<WIDTH, HEIGHT>
        { type TSprite = TSprite; }
    
        impl<TSprite, const WIDTH: u8, const HEIGHT: u8> IsTiledMarker for TiledMarker<TSprite, WIDTH, HEIGHT>
            where TSprite: Sprite<Tiled=TiledMarker<TSprite, WIDTH, HEIGHT>> + Tiled<WIDTH, HEIGHT>
        {}
    
        impl<TSprite> Sealed for NonTiledMarker<TSprite>
            where TSprite: Sprite<Tiled=NonTiledMarker<TSprite>> + NonTiled
        { type TSprite = TSprite; }
    
        impl<TSprite> IsTiledMarker for NonTiledMarker<TSprite>
            where TSprite: Sprite<Tiled=NonTiledMarker<TSprite>> + NonTiled
        {}
    }
}

pub mod traits {
    use std::ops::{Deref, DerefMut};
    use super::sealed::{IsStaticMarker, IsTiledMarker};

    pub trait Static {}
    pub trait Animated {
        type TileSprite: Sprite;
        fn get_frames(&self) -> u8;
        fn get_frame(&self, index: u8) -> Option<&Self::TileSprite>;

    }


    pub trait AnimatedMut : Animated {
        fn get_frame_mut(&mut self, index: u8) -> Option<&mut Self::TileSprite>;

    }
        
    pub trait Tiled<const WIDTH: u8, const HEIGHT: u8> {
        type TileSprite: Sprite+Tiled<1,1>;
        type TileSpriteDeref<'a>: 'a + Deref<Target=Self::TileSprite> where Self::TileSprite: 'a, Self: 'a;
        const BIAS_WIDTH: i8 = 0;
        const BIAS_HEIGHT: i8 = 0;

        fn get_centre(&self) -> &Self::TileSprite {
            let (x, y) = super::Cardinal::Centre.to_xy(WIDTH, HEIGHT, Self::BIAS_WIDTH, Self::BIAS_HEIGHT);
            self.get_indexed(x, y).expect("Bias so large that centre is out of bounds")
        }
        
        fn get_cardinal(&self, cardinal: super::Cardinal) -> Option<&Self::TileSprite> {
            let (x, y) = cardinal.to_xy(WIDTH, HEIGHT, Self::BIAS_WIDTH, Self::BIAS_HEIGHT);
            self.get_indexed(x, y)
        }
        
        fn get_centred(&self, x: i8, y: i8) -> Option<&Self::TileSprite> {
            self.get_indexed((x as i16 + (WIDTH / 2) as i16) as u8, (y as i16 + (HEIGHT / 2) as i16) as u8)
        }
        
        fn get_indexed(&self, x: u8, y: u8) -> Option<&Self::TileSprite> {
            if x < WIDTH && y < HEIGHT {
                unsafe { Some(self.get_indexed_unchecked(x, y)) }
            } else {
                None
            }
        }
        unsafe fn get_indexed_unchecked<'a>(&'a self, x: u8, y: u8) -> &Self::TileSprite;
    }


    pub trait TiledMut<const WIDTH: u8, const HEIGHT: u8> : Tiled<WIDTH, HEIGHT> {
        type TileSpriteDerefMut<'a>: DerefMut<Target=Self::TileSprite> where Self::TileSprite: 'a, Self: 'a;
        fn get_centre_mut(&mut self) -> &mut Self::TileSprite {
            let (x, y) = super::Cardinal::Centre.to_xy(WIDTH, HEIGHT, Self::BIAS_WIDTH, Self::BIAS_HEIGHT);
            self.get_indexed_mut(x, y).expect("Bias so large that centre is out of bounds")
        }

        fn get_cardinal_mut(&mut self, cardinal: super::Cardinal) -> Option<&mut Self::TileSprite> {
            let (x, y) = cardinal.to_xy(WIDTH, HEIGHT, Self::BIAS_WIDTH, Self::BIAS_HEIGHT);
            self.get_indexed_mut(x, y)
        }

        fn get_centred_mut(&mut self, x: i8, y: i8) -> Option<&mut Self::TileSprite> {
            self.get_indexed_mut((x as i16 + (WIDTH / 2) as i16) as u8, (y as i16 + (HEIGHT / 2) as i16) as u8)
        }

        fn get_indexed_mut(&mut self, x: u8, y: u8) -> Option<&mut Self::TileSprite> {
            if x < WIDTH && y < HEIGHT {
                unsafe { Some(self.get_indexed_mut_unchecked(x, y)) }
            } else {
                None
            }
        }

        unsafe fn get_indexed_mut_unchecked<'a>(&'a mut self, x: u8, y: u8) -> &mut Self::TileSprite;
    }

   /* pub trait SingleTile {}
    impl<TTiled : Tiled<1, 1>> SingleTile for TTiled{}*/

    pub trait NonTiled {}

    pub trait Sprite {
        type Tiled: IsTiledMarker;
        type Static: IsStaticMarker;
    }
}


pub enum Cardinal {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
    Centre
}

impl Cardinal {
    const fn to_vec(&self) -> (i8, i8) {
        match self {
            Cardinal::North => (0, -1),
            Cardinal::NorthEast => (1, -1),
            Cardinal::East => (1, 0),
            Cardinal::SouthEast => (1, 1),
            Cardinal::South => (0, 1),
            Cardinal::SouthWest => (-1, 1),
            Cardinal::West => (-1, 0),
            Cardinal::NorthWest => (-1, -1),
            Cardinal::Centre => (0, 0)
        }
    }

    const fn to_xy(&self, width: u8, height: u8, bias_width: i8, bias_height: i8) -> (u8, u8) {

        macro_rules! w {($bias:literal) => ((((width / 2) as i16 + bias_width as i16) + $bias) as u8)}
        macro_rules! h {($bias:literal) => ((((height / 2) as i16 + bias_height as i16) + $bias) as u8)}

        match self {
            Cardinal::North => (w!(0), h!(-1)),
            Cardinal::NorthEast => (w!(1), h!(-1)),
            Cardinal::East => (w!(1), h!(0)),
            Cardinal::SouthEast => (w!(1), h!(1)),
            Cardinal::South => (w!(0), h!(1)),
            Cardinal::SouthWest => (w!(-1), h!(1)),
            Cardinal::West => (w!(-1), h!(0)),
            Cardinal::NorthWest => (w!(-1), h!(-1)),
            Cardinal::Centre => (w!(0), h!(0))
        }
    }
    
}

pub struct StaticTileSprite{}

impl Sprite for StaticTileSprite{
    type Tiled = TiledMarker<Self, 1, 1>;
    type Static = StaticMarker<Self>;
}

impl Static for StaticTileSprite{}

impl Tiled<1,1> for StaticTileSprite{
    type TileSprite = Self;
    type TileSpriteDeref<'a> = &'a Self;

    unsafe fn get_indexed_unchecked(&self, _x: u8, _y: u8) -> Self::TileSpriteDeref<'_> {
        self
    }
}

impl TiledMut<1,1> for StaticTileSprite{
    type TileSpriteDerefMut<'a> = &'a mut Self;

    unsafe fn get_indexed_mut_unchecked(&mut self, _x: u8, _y: u8) -> Self::TileSpriteDerefMut<'_> {
        self
    }
}

pub struct Overlap1Sprite<TSprite>
where TSprite : Sprite<Tiled=TiledMarker<TSprite, 1, 1>, Static=StaticMarker<TSprite>> + Tiled<1, 1> + Static {
    sprites: [[TSprite; 3]; 3]
}

impl<TSprite> Sprite for Overlap1Sprite<TSprite>
    where TSprite : Sprite<Tiled=TiledMarker<TSprite, 1, 1>, Static=StaticMarker<TSprite>> + Tiled<1, 1> + Static {
    type Tiled = TiledMarker<Self, 3, 3>;
    type Static = StaticMarker<Self>;
}

impl<TSprite> Static for Overlap1Sprite<TSprite>
    where TSprite : Sprite<Tiled=TiledMarker<TSprite, 1, 1>, Static=StaticMarker<TSprite>> + Tiled<1, 1> + Static {}
impl<TSprite> Tiled<3,3> for Overlap1Sprite<TSprite>
    where TSprite : Sprite<Tiled=TiledMarker<TSprite, 1, 1>, Static=StaticMarker<TSprite>> + Tiled<1, 1> + Static {
    type TileSprite = TSprite;
    type TileSpriteDeref<'b> = &'b TSprite where TSprite: 'b;

    unsafe fn get_indexed_unchecked<'a>(&'a self, x: u8, y: u8) -> Self::TileSpriteDeref<'a> {
        self.sprites.get_unchecked(x as usize).get_unchecked(y as usize)
    }
}

impl<TSprite> TiledMut<3,3> for Overlap1Sprite<TSprite>
    where TSprite : Sprite<Tiled=TiledMarker<TSprite, 1, 1>, Static=StaticMarker<TSprite>> + TiledMut<1, 1> + Static {
    type TileSpriteDerefMut<'b> = &'b mut TSprite where TSprite: 'b;
    unsafe fn get_indexed_mut_unchecked(&mut self, x: u8, y: u8) -> &mut Self::TileSprite {
        self.sprites.get_unchecked_mut(x as usize).get_unchecked_mut(y as usize)
    }
}



pub struct AnimatedSprite<'a, TFrame, TIsTiled>
    where TFrame : 'a + Sprite<Tiled=TIsTiled, Static=StaticMarker<TFrame>> + Static {
    frames: Box<[TFrame]>,
    _phantom: PhantomData<&'a TIsTiled>
}

impl<'a, TFrame, TIsTiled> Sprite for AnimatedSprite<'a, TFrame, TIsTiled>
    where TFrame : Sprite<Tiled=TIsTiled, Static=StaticMarker<TFrame>> + Static,
      TIsTiled : sealed::IsTiledMarker {
    type Tiled = TIsTiled;
    type Static = sealed::AnimatedMarker<Self>;
}

impl <'a, TFrame, TIsTiled> Animated for AnimatedSprite<'a, TFrame, TIsTiled>
    where TFrame : Sprite<Tiled=TIsTiled, Static=StaticMarker<TFrame>> + Static {
    type TileSprite = TFrame;

    fn get_frames(&self) -> u8 {
        self.frames.len() as u8
    }

    fn get_frame(&self, index: u8) -> Option<&Self::TileSprite> {
        self.frames.get(index as usize)
    }
}

impl <'a, TFrame, TIsTiled> AnimatedMut for AnimatedSprite<'a, TFrame, TIsTiled>
    where TFrame : Sprite<Tiled=TIsTiled, Static=StaticMarker<TFrame>> + Static {

    fn get_frame_mut(&mut self, index: u8) -> Option<&mut Self::TileSprite> {
        self.frames.get_mut(index as usize)
    }
}


/*struct AnimatedTileSpriteStackAsRef<'a, TFrame, TSingle, const WIDTH: u8, const HEIGHT: u8>
    where TFrame : Sprite<Tiled=TiledMarker<TFrame, WIDTH, HEIGHT>, Static=StaticMarker<TFrame>> + Static + Tiled<WIDTH, HEIGHT, TileSprite=TSingle>,
          TSingle : Sprite + Tiled<1, 1>{
    frames: &'a AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>>
}

impl <'a, TFrame, TSingle, const WIDTH: u8, const HEIGHT: u8> AsRef<AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>> 
    for AnimatedTileSpriteStackBorrow<'a, TFrame, TSingle, WIDTH, HEIGHT>
    where TFrame : 'a + Sprite<Tiled=TiledMarker<TFrame, WIDTH, HEIGHT>, Static=StaticMarker<TFrame>> + Static + Tiled<WIDTH, HEIGHT, TileSprite=TSingle>,
          TSingle : Sprite + Tiled<1, 1> {}*/

/*
struct AnimatedTileSpriteStackRef<'a, TFrame, TSingle, TAnimated, const WIDTH: u8, const HEIGHT: u8>
    where TFrame : 'a + Sprite<Tiled=TiledMarker<TFrame, WIDTH, HEIGHT>, Static=StaticMarker<TFrame>> + Static + Tiled<WIDTH, HEIGHT, TileSprite=TSingle>,
          TSingle : Sprite + Tiled<1, 1>,
          TAnimated : Borrow<AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>>> {
    frames: TAnimated,
    x: u8,
    y: u8,
    _phantom_frame: PhantomData<&'a TFrame>,
    _phantom_single: PhantomData<&'a TSingle>
}

impl <'a, TFrame, TSingle, const WIDTH: u8, const HEIGHT: u8> Into<AnimatedTileSpriteStackRef<'a, TFrame, TSingle, &'a AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>>, WIDTH, HEIGHT>>
for AnimatedTileSpriteStackRef<'a, TFrame, TSingle, &'a mut AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>>, WIDTH, HEIGHT>
    where TFrame : Sprite<Tiled=TiledMarker<TFrame, WIDTH, HEIGHT>, Static=StaticMarker<TFrame>> + Static + Tiled<WIDTH, HEIGHT, TileSprite=TSingle>,
          TSingle : Sprite + Tiled<1, 1> {
    fn into(self) -> AnimatedTileSpriteStackRef<'a, TFrame, TSingle, &'a AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>>, WIDTH, HEIGHT> {
        let a = self.frames;
        AnimatedTileSpriteStackRef {
            frames: a,
            x: self.x,
            y: self.y,
            _phantom_frame: Default::default(),
            _phantom_single: Default::default(),
        }

    }
}

impl <'a, TFrame, TSingle, TAnimated, const WIDTH: u8, const HEIGHT: u8> Sprite for AnimatedTileSpriteStackRef<'a, TFrame, TSingle, TAnimated, WIDTH, HEIGHT>
    where TFrame : Sprite<Tiled=TiledMarker<TFrame, WIDTH, HEIGHT>, Static=StaticMarker<TFrame>> + Static + Tiled<WIDTH, HEIGHT, TileSprite=TSingle>,
          TSingle : Sprite + Tiled<1, 1>,
          TAnimated : Borrow<AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>>> {
    type Tiled = TiledMarker<Self, 1, 1>;
    type Static = StaticMarker<Self>;
}

impl <'a, TFrame, TSingle, TAnimated, const WIDTH: u8, const HEIGHT: u8> Static for AnimatedTileSpriteStackRef<'a, TFrame, TSingle, TAnimated, WIDTH, HEIGHT>
    where TFrame : Sprite<Tiled=TiledMarker<TFrame, WIDTH, HEIGHT>, Static=StaticMarker<TFrame>> + Static + Tiled<WIDTH, HEIGHT, TileSprite=TSingle>,
          TSingle : Sprite + Tiled<1, 1>,
          TAnimated : Borrow<AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>>> {}

impl <'a, TFrame, TSingle, TAnimated, const WIDTH: u8, const HEIGHT: u8> Tiled<1, 1> for AnimatedTileSpriteStackRef<'a, TFrame, TSingle, TAnimated, WIDTH, HEIGHT>
    where TFrame : Sprite<Tiled=TiledMarker<TFrame, WIDTH, HEIGHT>, Static=StaticMarker<TFrame>> + Static + Tiled<WIDTH, HEIGHT, TileSprite=TSingle>,
          TSingle : Sprite + Tiled<1, 1>,
          TAnimated : Borrow<AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>>> {
    type TileSprite = Self;
    type TileSpriteDeref<'b> = &'b Self where <Self as Tiled<1, 1>>::TileSprite: 'b, Self: 'b;

    unsafe fn get_indexed_unchecked<'b>(&'b self, _x: u8, _y: u8) -> Self::TileSpriteDeref<'b> {
        self
    }
}

impl <'a, TFrame, TSingle, TAnimated, const WIDTH: u8, const HEIGHT: u8> Deref for AnimatedTileSpriteStackRef<'a, TFrame, TSingle, TAnimated, WIDTH, HEIGHT>
    where TFrame : Sprite<Tiled=TiledMarker<TFrame, WIDTH, HEIGHT>, Static=StaticMarker<TFrame>> + Static + Tiled<WIDTH, HEIGHT, TileSprite=TSingle>,
          TSingle : Sprite + Tiled<1, 1>,
          TAnimated : Borrow<AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>>> {
    type Target = Self;

    fn deref(&self) -> &Self::Target {
        self
    }
}

impl <'a, TFrame, TSingle, TAnimated, const WIDTH: u8, const HEIGHT: u8> DerefMut for AnimatedTileSpriteStackRef<'a, TFrame, TSingle, TAnimated, WIDTH, HEIGHT>
    where TFrame : Sprite<Tiled=TiledMarker<TFrame, WIDTH, HEIGHT>, Static=StaticMarker<TFrame>> + Static + Tiled<WIDTH, HEIGHT, TileSprite=TSingle>,
          TSingle : Sprite + Tiled<1, 1>,
          TAnimated : BorrowMut<AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>>> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self
    }
}

impl <'a, TFrame, TSingle, TAnimated, const WIDTH: u8, const HEIGHT: u8> Animated for AnimatedTileSpriteStackRef<'a, TFrame, TSingle, TAnimated, WIDTH, HEIGHT>
    where TFrame : Sprite<Tiled=TiledMarker<TFrame, WIDTH, HEIGHT>, Static=StaticMarker<TFrame>> + Static + Tiled<WIDTH, HEIGHT, TileSprite=TSingle>,
          TSingle : Sprite + Tiled<1, 1>,
          TAnimated : Borrow<AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>>> {
    type TileSprite = TSingle;

    fn get_frames(&self) -> u8 {
        self.frames.borrow().get_frames()
    }

    fn get_frame(&self, index: u8) -> Option<&Self::TileSprite> {
        self.frames.borrow().get_frame(index).map(|frame| unsafe{frame.get_indexed_unchecked(self.x, self.y).deref()})
    }
}

impl <'a, TFrame, TSingle, TAnimated, const WIDTH: u8, const HEIGHT: u8> AnimatedMut for AnimatedTileSpriteStackRef<'a, TFrame, TSingle, TAnimated, WIDTH, HEIGHT>
    where TFrame : Sprite<Tiled=TiledMarker<TFrame, WIDTH, HEIGHT>, Static=StaticMarker<TFrame>> + Static + TiledMut<WIDTH, HEIGHT, TileSprite=TSingle>,
          TSingle : Sprite + Tiled<1, 1>,
          TAnimated : BorrowMut<AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>>> {
    fn get_frame_mut(&mut self, index: u8) -> Option<&mut Self::TileSprite> {
        self.frames.borrow_mut().get_frame_mut(index).map(|frame| unsafe{frame.get_indexed_mut_unchecked(self.x, self.y).deref_mut()})
    }
}



impl <'a, TFrame, TSingle, const WIDTH: u8, const HEIGHT: u8> Tiled<WIDTH, HEIGHT> for &'a AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>>
    where TFrame : 'a + Sprite<Tiled=TiledMarker<TFrame, WIDTH, HEIGHT>, Static=StaticMarker<TFrame>> + Static + Tiled<WIDTH, HEIGHT, TileSprite=TSingle>,
          TSingle : 'a + Sprite + Tiled<1, 1> {
    type TileSprite = AnimatedTileSpriteStackRef<'a, TFrame, TSingle, Self, WIDTH, HEIGHT>;
    type TileSpriteDeref<'b>= Self::TileSprite where <Self as Tiled<WIDTH, HEIGHT>>::TileSprite: 'b, Self: 'b;

    unsafe fn get_indexed_unchecked<'b>(&'b self, x: u8, y: u8) -> Self::TileSpriteDeref<'b> {
        AnimatedTileSpriteStackRef {
            frames: *self,
            x,
            y,
            _phantom_frame: Default::default(),
            _phantom_single: Default::default(),
        }
    }
}

impl <'a, TFrame, TSingle, const WIDTH: u8, const HEIGHT: u8> Tiled<WIDTH, HEIGHT> for &'a mut AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>>
    where TFrame : 'a + Sprite<Tiled=TiledMarker<TFrame, WIDTH, HEIGHT>, Static=StaticMarker<TFrame>> + Static + Tiled<WIDTH, HEIGHT, TileSprite=TSingle>,
          TSingle : 'a + Sprite + Tiled<1, 1> {
    type TileSprite = AnimatedTileSpriteStackRef<'a, TFrame, TSingle, Self, WIDTH, HEIGHT>;
    type TileSpriteDeref<'b>= Self::TileSprite where <Self as Tiled<WIDTH, HEIGHT>>::TileSprite: 'b, Self: 'b;

    unsafe fn get_indexed_unchecked<'b>(&'b self, x: u8, y: u8) -> Self::TileSpriteDeref<'b> {
        AnimatedTileSpriteStackRef {
            frames: self,
            x,
            y,
            _phantom_frame: Default::default(),
            _phantom_single: Default::default(),
        }
    }
}
impl <'a, TFrame, TSingle, const WIDTH: u8, const HEIGHT: u8> TiledMut<WIDTH, HEIGHT> for &'a mut AnimatedSprite<'a, TFrame, TiledMarker<TFrame, WIDTH, HEIGHT>>
    where TFrame : 'a + Sprite<Tiled=TiledMarker<TFrame, WIDTH, HEIGHT>, Static=StaticMarker<TFrame>> + Static + Tiled<WIDTH, HEIGHT, TileSprite=TSingle>,
          TSingle : 'a + Sprite + Tiled<1, 1> {
    type TileSpriteDerefMut<'b>= Self::TileSprite where <Self as Tiled<WIDTH, HEIGHT>>::TileSprite: 'b, Self: 'b;

    unsafe fn get_indexed_mut_unchecked<'b>(&'b mut self, x: u8, y: u8) -> Self::TileSpriteDerefMut<'b> {
        AnimatedTileSpriteStackRef {
            frames: *self,
            x,
            y,
            _phantom_frame: Default::default(),
            _phantom_single: Default::default(),
        }
    }
}*/

impl <'a, TFrame> NonTiled for AnimatedSprite<'a, TFrame, NonTiledMarker<TFrame>>
    where TFrame : Sprite<Tiled=NonTiledMarker<TFrame>, Static=StaticMarker<TFrame>> + Static + NonTiled {}