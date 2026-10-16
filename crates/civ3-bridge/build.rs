fn main() {
    let def = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("ifc23.def");
    println!("cargo::rerun-if-changed={}", def.display());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo::rustc-cdylib-link-arg={}", def.display());
    }
}
