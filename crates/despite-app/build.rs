#![allow(non_snake_case)]

fn main() {
    let Icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../platform/windows/despite.ico");
    println!("cargo:rerun-if-changed={}", Icon.display());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") || !Icon.exists() {
        return;
    }
    let mut Resource = winresource::WindowsResource::new();
    Resource.set_icon(&Icon.to_string_lossy());
    Resource.set("ProductName", "Despite");
    Resource.set("FileDescription", "Despite art program");
    Resource.set("CompanyName", "Despite contributors");
    if let Err(Problem) = Resource.compile() {
        println!("cargo:warning=could not embed the Windows icon: {Problem}");
    }
}
