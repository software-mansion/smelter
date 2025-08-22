use std::sync::{
    atomic::{AtomicU16, Ordering},
    OnceLock,
};

pub fn get_free_port() -> u16 {
    static LAST_PORT: OnceLock<AtomicU16> = OnceLock::new();
    let port =
        LAST_PORT.get_or_init(|| AtomicU16::new(10_000 + (rand::random::<u16>() % 5_000) * 2));
    port.fetch_add(2, Ordering::Relaxed)
}
