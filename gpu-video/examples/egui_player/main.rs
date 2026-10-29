#[cfg(supported)]
mod egui_player;

#[cfg(supported)]
fn main() {
    egui_player::run()
}

#[cfg(not(supported))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan or video toolbox"
    );
}
