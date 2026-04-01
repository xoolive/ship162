fn main() {
    // Set stack size to 8 MB on Windows MSVC to avoid stack overflows
    // during deep recursive parsing
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("msvc") {
        println!("cargo:rustc-link-arg=/stack:8388608");
    }
}
