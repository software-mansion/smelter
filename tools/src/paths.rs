use std::path::PathBuf;

pub fn git_root() -> PathBuf {
    tools_root().parent().unwrap().to_path_buf()
}

pub fn tools_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
