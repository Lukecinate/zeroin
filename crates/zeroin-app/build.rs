fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/brand/zeroin.ico");
        res.set("ProductName", "ZeroIn");
        res.set("FileDescription", "ZeroIn sensitivity finder");
        res.set("LegalCopyright", "Copyright (c) 2026 Rozan Abid. MIT License.");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon resource not embedded: {e}");
        }
    }
    println!("cargo:rerun-if-changed=../../assets/brand/zeroin.ico");
}
