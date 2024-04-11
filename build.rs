use spirv_builder::{MetadataPrintout, SpirvBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    dbg!("Building shaders");
    SpirvBuilder::new("./shaders", "spirv-unknown-vulkan1.2")
        .print_metadata(MetadataPrintout::Full)
        .release(true)
        .build()?;
    Ok(())
}