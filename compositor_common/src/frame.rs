use crate::scene::Resolution;

#[derive(Debug)]
pub struct Frame {
    pub data: YuvData,
    pub resolution: Resolution,
    pub pts: i64,
}

#[derive(Debug)]
pub struct YuvData {
    pub y_plane: bytes::Bytes,
    pub u_plane: bytes::Bytes,
    pub v_plane: bytes::Bytes,
}
