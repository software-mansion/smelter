use std::sync::{
    atomic::{AtomicU16, Ordering},
    OnceLock,
};

pub const ELEPHANT_URL: &str =
    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ElephantsDream.mp4";
pub const ELEPHANT_PATH: &str = "examples/assets/ElephantsDream720p24fps654s.mp4";

pub const BUNNY_URL: &str =
    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4";
pub const BUNNY_PATH: &str = "examples/assets/BigBuckBunny720p24fps597s.mp4";

pub fn get_free_port() -> u16 {
    static LAST_PORT: OnceLock<AtomicU16> = OnceLock::new();
    let port =
        LAST_PORT.get_or_init(|| AtomicU16::new(10_000 + (rand::random::<u16>() % 5_000) * 2));
    port.fetch_add(2, Ordering::Relaxed)
}
