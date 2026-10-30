fn main() {
    #[cfg(target_os = "linux")]
    link_without_dev_packages(&["xkbcommon-x11", "xkbcommon", "xcb", "freetype"]);
}

#[cfg(target_os = "linux")]
fn link_without_dev_packages(libraries: &[&str]) {
    use std::path::{Path, PathBuf};
    let dirs = ["/usr/lib/x86_64-linux-gnu", "/usr/lib/aarch64-linux-gnu", "/usr/lib64", "/usr/lib"];
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let mut linked = false;
    for library in libraries {
        let dev = format!("lib{library}.so");
        if dirs.iter().any(|d| Path::new(d).join(&dev).exists()) {
            continue;
        }
        let runtime = dirs
            .iter()
            .map(|d| Path::new(d).join(format!("{dev}.0")))
            .chain(dirs.iter().map(|d| Path::new(d).join(format!("{dev}.6"))))
            .find(|p| p.exists());
        if let Some(runtime) = runtime {
            let link = out.join(&dev);
            let _ = std::fs::remove_file(&link);
            if std::os::unix::fs::symlink(&runtime, &link).is_ok() {
                linked = true;
            }
        }
    }
    if linked {
        println!("cargo:rustc-link-search=native={}", out.display());
    }
}
