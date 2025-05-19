use std::{path::Path, sync::Arc};

#[derive(Debug, Clone)]
pub struct Mp4InputOptions {
    pub source: Mp4InputSource,
    pub should_loop: bool,
}

#[derive(Debug, Clone)]
pub enum Mp4InputSource {
    Url(Arc<str>),
    File(Arc<Path>),
}
