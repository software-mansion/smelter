@group(0) @binding(0) var source_y: texture_storage_2d<r8unorm, read>;
@group(0) @binding(1) var source_uv: texture_storage_2d<rg8unorm, read>;

@group(1) @binding(0) var dest_y: binding_array<texture_storage_2d<r8unorm, write>, 8>;
@group(2) @binding(0) var dest_uv: binding_array<texture_storage_2d<rg8unorm, write>, 8>;

@group(3) @binding(0) var<storage, read_write> debug_buf: array<u32>;

var<immediate> output_number: u32;

@compute
@workgroup_size(256)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
  var remaining_offset: u32 = id.x;
  var i: u32 = 0;
  for ( ; i < output_number; i++ ) {
    let size = textureDimensions(dest_y[i]);
    let total_size = size.x * size.y;
    if (remaining_offset < total_size) {
      break;
    }

    remaining_offset -= total_size;
  }

  let size = textureDimensions(dest_y[i]);
  let y = remaining_offset / size.x;
  let x = remaining_offset % size.x;
  let coords_output = vec2(x, y);

  if y >= size.y {
    return;
  }

  let float_coords = (vec2<f32>(coords_output) + 0.5) / vec2<f32>(size);
  let input_size = textureDimensions(source_y);
  let coords_input = vec2<u32>(vec2<f32>(input_size) * float_coords);

  // Debug: per-output dimensions (written by first thread of each output)
  if (x == 0u && y == 0u) {
    let base = i * 8u;
    let uv_size = textureDimensions(dest_uv[i]);
    let source_uv_size = textureDimensions(source_uv);
    debug_buf[base + 0u] = size.x;
    debug_buf[base + 1u] = size.y;
    debug_buf[base + 2u] = uv_size.x;
    debug_buf[base + 3u] = uv_size.y;
    debug_buf[base + 4u] = input_size.x;
    debug_buf[base + 5u] = input_size.y;
    debug_buf[base + 6u] = source_uv_size.x;
    debug_buf[base + 7u] = source_uv_size.y;
  }

  let input_y = textureLoad(source_y, coords_input);
  textureStore(dest_y[i], coords_output, input_y);

  // Debug: Y values for bottom 16 rows of last output
  if (i == output_number - 1u && y >= size.y - 16u) {
    let debug_row = y - (size.y - 16u);
    let pixel_offset = 64u + (debug_row * size.x + x);
    if (pixel_offset < arrayLength(&debug_buf)) {
      debug_buf[pixel_offset] = bitcast<u32>(input_y.x);
    }
  }

  if (x % 2 == 0 && y % 2 == 0) {
    let input_uv = textureLoad(source_uv, coords_input / 2);
    textureStore(dest_uv[i], coords_output / 2, input_uv);

    // Debug: UV values for bottom 16 rows of last output
    if (i == output_number - 1u && y >= size.y - 16u) {
      let debug_row = (y - (size.y - 16u)) / 2u;
      let uv_offset = 64u + 16u * size.x + (debug_row * (size.x / 2u) + x / 2u) * 2u;
      if (uv_offset + 1u < arrayLength(&debug_buf)) {
        debug_buf[uv_offset] = bitcast<u32>(input_uv.x);
        debug_buf[uv_offset + 1u] = bitcast<u32>(input_uv.y);
      }
    }
  }
}
