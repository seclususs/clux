use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=CLUX_LIB_DIR");
    if let Some(dir) = env::var_os("CLUX_LIB_DIR") {
        let dir = dir.to_string_lossy();
        println!("cargo:rustc-link-search=native={dir}");
        println!("cargo:rerun-if-changed={dir}/libclux.a");
    }
}
