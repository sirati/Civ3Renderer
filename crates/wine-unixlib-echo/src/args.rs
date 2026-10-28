/// The argument of every echo call: `output` gets `input` reversed, and
/// `value` becomes `value * 3 + code`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Echo {
    pub input: wine_unixlib::Buffer32,
    pub output: wine_unixlib::Buffer32,
    pub value: u32,
    pub _pad: u32,
}

/// How many calls the unixlib has.
pub const CALLS: u32 = 20;
