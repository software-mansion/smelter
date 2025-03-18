use std::{env, path::PathBuf};

pub fn get_smelter_instance_tmp_path(compositor_instance_id: &str) -> PathBuf {
    env::temp_dir()
        .join("smelter")
        .join(format!("instance_{compositor_instance_id}"))
}
