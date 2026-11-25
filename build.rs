fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=migrations");
    if std::env::var_os("CARGO_FEATURE_EMBEDDED_ADMIN").is_some() {
        println!("cargo:rerun-if-changed=admin/dist");
        assert!(
            std::path::Path::new("admin/dist/index.html").is_file(),
            "build admin/dist with npm run build before enabling embedded-admin"
        );
    }
    println!(
        "cargo:rustc-env=GATEWAY_BUILD_TARGET={}",
        std::env::var("TARGET").unwrap()
    );
}
