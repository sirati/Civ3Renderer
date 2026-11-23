fn main() {
    let def = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("sound.def");
    println!("cargo::rerun-if-changed={}", def.display());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo::rustc-cdylib-link-arg=-Wl,--enable-stdcall-fixup");
        println!("cargo::rustc-cdylib-link-arg={}", def.display());
    }
}
