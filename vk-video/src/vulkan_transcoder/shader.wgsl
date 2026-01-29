@group(0) @binding(0) var source_y: texture_storage_2d<r8unorm, read>;
@group(0) @binding(1) var source_uv: texture_storage_2d<rg8unorm, read>;

@group(1) @binding(0) var dest_y: binding_array<texture_storage_2d<r8unorm, write> >;
@group(2) @binding(0) var dest_uv: binding_array<texture_storage_2d<rg8unorm, write> >;

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

  let float_coords = vec2<f32>(coords_output) / vec2<f32>(size);
  let input_size = textureDimensions(source_y);
  let coords_input = vec2<u32>(vec2<f32>(input_size) * float_coords);

  let input_y = textureLoad(source_y, coords_input);
  textureStore(dest_y[i], coords_output, input_y);

  let input_uv = textureLoad(source_uv, coords_input / 2);
  textureStore(dest_uv[i], coords_output / 2, input_uv);
}
