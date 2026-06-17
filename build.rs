use std::env;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os != "linux" {
        println!(
            "cargo:warning=Pomidoro is designed for Linux. It is very unlikely to work on systems not offering Linux-like abstract sockets and filesystem structure."
        );
    }
}
