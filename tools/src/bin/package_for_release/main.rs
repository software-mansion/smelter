#[cfg(target_os = "linux")]
mod bundle_linux;
#[cfg(target_os = "macos")]
mod bundle_macos;
mod utils;

#[cfg(target_os = "linux")]
fn main() {
    bundle_linux::bundle_linux_app().unwrap();
}

#[cfg(target_os = "macos")]
fn main() {
    bundle_macos::bundle_macos_app().unwrap();
}
