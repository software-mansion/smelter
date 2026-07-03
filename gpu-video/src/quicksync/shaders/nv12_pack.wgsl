struct Params {
    y_rows: u32,
    visible_width: u32,
    visible_height: u32,
    _pad: u32,
}

@group(0) @binding(0) var y_plane: texture_2d<f32>;
@group(0) @binding(1) var uv_plane: texture_2d<f32>;
@group(0) @binding(2) var planes: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<uniform> params: Params;

// Repacks a caller NV12 texture into the coded surface, which is imported as
// one quarter-width RGBA8 image (Y rows, then interleaved UV rows). Reads
// clamp to the visible frame so alignment padding is edge-replicated.

@compute @workgroup_size(8, 8)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let x = i32(gid.x);
    let y = i32(gid.y);
    if y < i32(params.y_rows) {
        let sy = min(y, i32(params.visible_height) - 1);
        let sx = 4 * x;
        let last = i32(params.visible_width) - 1;
        textureStore(planes, vec2(x, y), vec4(
            textureLoad(y_plane, vec2(min(sx, last), sy), 0).r,
            textureLoad(y_plane, vec2(min(sx + 1, last), sy), 0).r,
            textureLoad(y_plane, vec2(min(sx + 2, last), sy), 0).r,
            textureLoad(y_plane, vec2(min(sx + 3, last), sy), 0).r,
        ));
    } else {
        let cy = min(y - i32(params.y_rows), i32(params.visible_height) / 2 - 1);
        let sx = 2 * x;
        let last = i32(params.visible_width) / 2 - 1;
        let left = textureLoad(uv_plane, vec2(min(sx, last), cy), 0).rg;
        let right = textureLoad(uv_plane, vec2(min(sx + 1, last), cy), 0).rg;
        textureStore(planes, vec2(x, y), vec4(left, right));
    }
}
