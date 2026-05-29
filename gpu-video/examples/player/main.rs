#[cfg(vulkan)]
mod player;

#[cfg(vulkan)]
#[tokio::main]
async fn main() {
    player::run().await
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
