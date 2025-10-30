use anyhow::Result;
use std::{
    env, fs,
    path::PathBuf,
    sync::{
        OnceLock,
        atomic::{AtomicU16, Ordering},
    },
};

pub fn get_free_port() -> u16 {
    static LAST_PORT: OnceLock<AtomicU16> = OnceLock::new();
    let port =
        LAST_PORT.get_or_init(|| AtomicU16::new(10_000 + (rand::random::<u16>() % 5_000) * 2));
    port.fetch_add(2, Ordering::Relaxed)
}

pub fn resolve_path(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        let cwd = env::current_dir()?;

        Ok(cwd.join(path))
    }
}

pub fn parse_json(json_path: PathBuf) -> Result<serde_json::Value> {
    let json_str = fs::read_to_string(json_path)?;
    Ok(serde_json::from_str(&json_str)?)
}
