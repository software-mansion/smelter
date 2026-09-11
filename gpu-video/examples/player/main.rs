#[cfg(supported)]
mod player;

#[cfg(supported)]
fn main() {
    player::run()
}

#[cfg(not(supported))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan or video toolbox"
    );
}
