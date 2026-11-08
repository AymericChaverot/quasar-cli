//! Passes the target triple to the code, which `qsr update` needs to pick the
//! release archive built for the platform it runs on.

fn main() {
    let target = std::env::var("TARGET").expect("cargo sets TARGET");
    println!("cargo:rustc-env=QSR_TARGET={target}");
    println!("cargo:rerun-if-changed=build.rs");
}
