fn main() {
    println!("cargo:rerun-if-changed=../../assets/moca_icon.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/moca_icon.ico");
        res.compile()
            .expect("failed to compile Windows icon resource for Moca");
    }
}
