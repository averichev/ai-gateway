fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=release.config.json");
    println!("cargo:rerun-if-changed=migrations");
    let identity: serde_json::Value = serde_json::from_str(include_str!("release.config.json"))
        .expect("release.config.json must contain valid JSON");
    let product_id = identity["product_id"]
        .as_str()
        .expect("release.config.json must contain a product_id string");
    let public_key = identity
        .pointer("/updates/public_key")
        .expect("release.config.json must contain updates.public_key");
    println!("cargo:rustc-env=GATEWAY_PRODUCT_ID={product_id}");
    println!("cargo:rustc-env=GATEWAY_UPDATE_PUBLIC_KEY={public_key}");
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
