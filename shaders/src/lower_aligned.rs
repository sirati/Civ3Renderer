use core::hint::unreachable_unchecked;
use core::marker::PhantomData;

pub trait NamedArray<'a> : Copy {
    fn inner_as_ref(&self) -> &'a [u32];
    //fn inner_as_mut(&mut self) -> &mut [u32];
}

//macro to create NewType of [u32] implementing NamedArray<'a>


fn reinterpret_cast<'a, T>(a: &'a [u32]) -> &'a T
    where &'a T : NamedArray<'a> {
    unsafe {
        &*(a as *const [u32] as *const T)

    }
}

#[macro_export]
macro_rules! named_array {
    ($name:ident) => {
        #[repr(transparent)]
        #[derive(Copy, Clone)]
        pub struct $name<'a>(&'a [u32]);
        impl<'a> NamedArray<'a> for $name<'a> {
            fn inner_as_ref(&self) -> &'a[u32] {
                self.0
            }
        }
    };
}
/*#[macro_export]
macro_rules! named_array {
    ($name:ident) => {
        #[repr(transparent)]
        pub struct $name([u32]);
        impl<'a> NamedArray<'a> for &'a $name {
            fn inner_as_ref(&self) -> &'a [u32] {
                &(*self).0
            }
        }
    };
}*/

#[repr(transparent)]
pub struct U32AlignedIndex<'a, TArr: NamedArray<'a>>(usize, PhantomData<&'a TArr>);
#[repr(transparent)]
pub struct U8AlignedIndex<'a, TArr: NamedArray<'a>>(usize, PhantomData<&'a TArr>);
#[repr(transparent)]
pub struct U4AlignedIndex<'a, TArr: NamedArray<'a>>(usize, PhantomData<&'a TArr>);

pub trait AlignedIndex<'a, TArr: NamedArray<'a>, const N: usize> : Sized {
    fn new_u32(index: u32, _arr: TArr) -> Self {
        Self::_new_u32(index, PhantomData)
    }
    fn _new_u32(index: u32, _arr: PhantomData<TArr>) -> Self;

    fn _inner_mut(&mut self) -> &mut usize;
    fn _inner_ref(&self) -> &usize;
    
    fn offset(&self, offset: usize) -> Self {
        Self::_new_u32((self._inner_ref() + offset) as u32, PhantomData)
    }

    fn skip(&mut self, offset: usize) {
        let i = self._inner_mut();
        *i += offset;
    }
}




impl<'a, TArr: NamedArray<'a>> AlignedIndex<'a, TArr, 32> for U32AlignedIndex<'a, TArr> {

    fn _new_u32(index: u32, _arr: PhantomData<TArr>) -> Self {
        U32AlignedIndex((index << 2) as usize, PhantomData)
    }
    fn _inner_mut(&mut self) -> &mut usize {
        &mut self.0
    }
    fn _inner_ref(& self) -> &usize {
        &self.0
    }
}

impl<'a, TArr: NamedArray<'a>> AlignedIndex<'a, TArr, 8> for U8AlignedIndex<'a, TArr> {
    fn _new_u32(index: u32, _arr: PhantomData<TArr>) -> Self {
        U8AlignedIndex((index << 2) as usize, PhantomData)
    }
    fn _inner_mut(&mut self) -> &mut usize {
        &mut self.0
    }
    fn _inner_ref(& self) -> &usize {
        &self.0
    }
}

impl<'a, TArr: NamedArray<'a>> AlignedIndex<'a, TArr, 4> for U4AlignedIndex<'a, TArr> {
    fn _new_u32(index: u32, _arr: PhantomData<TArr>) -> Self {
        U4AlignedIndex((index << 2) as usize, PhantomData)
    }
    fn _inner_mut(&mut self) -> &mut usize {
        &mut self.0
    }
    fn _inner_ref(& self) -> &usize {
        &self.0
    }
}


impl<'a, TArr: NamedArray<'a>> From<U4AlignedIndex<'a, TArr>> for U8AlignedIndex<'a, TArr> {
    fn from(index: U4AlignedIndex<'a, TArr>) -> Self {
        U8AlignedIndex((index.0 + 0b1)>>1, PhantomData)
    }
}

impl<'a, TArr: NamedArray<'a>> From<U8AlignedIndex<'a, TArr>> for U32AlignedIndex<'a, TArr> {
    fn from(index: U8AlignedIndex<'a, TArr>) -> Self {
        U32AlignedIndex((index.0 + 0b11) >> 2, PhantomData)
    }
}

impl<'a, TArr: NamedArray<'a>> From<U4AlignedIndex<'a, TArr>> for U32AlignedIndex<'a, TArr> {
    fn from(index: U4AlignedIndex<'a, TArr>) -> Self {
        U32AlignedIndex((index.0 + 0b111) >> 3, PhantomData)
    }
}

impl<'a, TArr: NamedArray<'a>> From<U8AlignedIndex<'a, TArr>> for U4AlignedIndex<'a, TArr> {
    fn from(index: U8AlignedIndex<'a, TArr>) -> Self {
        U4AlignedIndex(index.0<<1, PhantomData)
    }
}

impl<'a, TArr: NamedArray<'a>> From<U32AlignedIndex<'a, TArr>> for U4AlignedIndex<'a, TArr> {
    fn from(index: U32AlignedIndex<'a, TArr>) -> Self {
        U4AlignedIndex(index.0 << 3, PhantomData)
    }
}

impl<'a, TArr: NamedArray<'a>> From<U32AlignedIndex<'a, TArr>> for U8AlignedIndex<'a, TArr> {
    fn from(index: U32AlignedIndex<'a, TArr>) -> Self {
        U8AlignedIndex(index.0 >> 2, PhantomData)
    }
}

impl<'a, TArr: NamedArray<'a>> U32AlignedIndex<'a, TArr> {

}

impl<'a, TArr: NamedArray<'a>> U4AlignedIndex<'a, TArr> {
  


    pub fn read_u4(&mut self, buffer: TArr) -> u32 {
        let value= self.get_u4(buffer);
        self.0 += 1;
        value
    }
    pub  fn get_u4(&self, buffer: TArr) -> u32 {
        let shift = ((self.0 & 0b111) ^ 0b111) << 2; 
        (buffer.inner_as_ref()[self.0 >> 3] >> shift) & 0x0F
    }
}


impl<'a, TArr: NamedArray<'a>> U32AlignedIndex<'a, TArr> {



    pub fn read_u32(&mut self, buffer: TArr) -> u32 {
        let value= self.get_u32(buffer);
        self.0 += 1;
        value
    }
    pub fn get_u32(&self, buffer: TArr) -> u32 {
        buffer.inner_as_ref()[self.0]
    }
}

impl<'a, TArr: NamedArray<'a>> U8AlignedIndex<'a, TArr> {


    pub fn read_u8(&mut self, buffer: TArr) -> u32 {
        let value= self.get_u8(buffer);
        self.0 += 1;
        value
    }
    pub fn get_u8(&self, buffer: TArr) -> u32 {
        let shift = ((self.0 & 0b11) ^ 0b11) << 3; // Calculate shift
        let value = (buffer.inner_as_ref()[self.0 >> 3] >> shift) & 0xFF; // Shift and mask
        value
    }

    pub fn read_u16(&mut self, buffer: TArr) -> u32 {
        let value= self.get_u16(buffer);
        self.0 += 2;
        value
        
    }
    pub fn get_u16(&self, buffer: TArr) -> u32 {
        let index = self.0 >> 2;
        //manual code
        //match hopefully realised by branching twice
        let (mask_high, shift_high) = match self.0 & 0b11 {
            0 => (0xFFFF0000, 16), /*it may be faster to calculate shift and only branch once */
            1 => (0x00FFFF00, 08), /*let shift =  (2 - (i & 0b11)) << 3;*/
            2 => (0x0000FFFF, 00),
            3 => { //we must do this separately as shift by neg num is UB
                return ((buffer.inner_as_ref()[index + 0] & 0x000000FF) << 8) |
                    ((buffer.inner_as_ref()[index + 1] & 0xFF000000) >> 24);
            },
            _ => unsafe {unreachable_unchecked()},
        };
        (buffer.inner_as_ref()[index + 0] & mask_high) >> shift_high
    }

    pub fn read_u24(&mut self, buffer: TArr) -> u32 {
        let value= self.get_u24(buffer);
        self.0 += 3;
        value
    }
    pub fn get_u24(&self, buffer: TArr) -> u32 {
        let index = self.0 >> 2;
        let need_two_reads = self.0 & 0b10;
        let low = self.0 & 0b01;
        if need_two_reads == 0 {
            //shift is 8 if low == 0 
            //shift is 0 if low == 1 
            (buffer.inner_as_ref()[index] >> ((low ^ 1)<<3)) & 0x00FFFFFF
        } else {
            let (mask_high, mask_low, shift_high, shift_low) = if low == 0 {
                (0x0000FFFF, 0xFF000000, 08, 24)
            } else {
                (0x000000FF, 0xFFFF0000, 16, 16)
            };
            ((buffer.inner_as_ref()[index + 0] & mask_high) << shift_high) |
                ((buffer.inner_as_ref()[index + 1] & mask_low) >> shift_low)
            
        }
    }


    pub fn read_u32(&mut self, buffer: TArr) -> u32 {
        let value= self.get_u32(buffer);
        self.0 += 4;
        value
    }
    pub fn get_u32(&self, buffer: TArr) -> u32 {
        let off_alignment = self.0 & 0b11;
        let index = self.0 >> 2;
        match off_alignment {
            0 => buffer.inner_as_ref()[index],
            1 => buffer.inner_as_ref()[index] << 8 | buffer.inner_as_ref()[index + 1] >> 24,
            2 => buffer.inner_as_ref()[index] << 16 | buffer.inner_as_ref()[index + 1] >> 16,
            3 => buffer.inner_as_ref()[index] << 24 | buffer.inner_as_ref()[index + 1] >> 8,
            _ => unsafe {unreachable_unchecked()},
        }
    }
    
    pub fn read_n_bytes(&mut self, buffer: TArr, n: u32) -> u32 {
        let value= self.get_n_bytes(buffer, n);
        self.0 += n as usize;
        value
    }
    pub fn get_n_bytes(&self, buffer: TArr, n: u32) -> u32 {
        match n {
            0 => self.get_u8(buffer),
            1 => self.get_u16(buffer),
            2 => self.get_u24(buffer),
            3 => self.get_u32(buffer),
            _ => panic!("ONLY <=4 BYTES SUPPORTED, n+1=Bytes read"),
        }
    }

    pub fn read_u24_high(&mut self, buffer: TArr, high: u32) -> u32 {
        let value= self.get_u24_high(buffer, high);
        self.0 += 3;
        value
    }
    pub fn get_u24_high(&self, buffer: TArr, high: u32) -> u32 {
        ((high & 0xFF) << 24) | (self.get_u24(buffer) << 8)
    }
}
