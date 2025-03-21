struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    output.position = vec4(input.position, 1.0);
    output.tex_coords = input.tex_coords;

    return output;
}

@group(0) @binding(0) var texture: texture_2d<f32>;
@group(1) @binding(0) var sampler_: sampler;

fn srgb_to_linear(srgb: vec3<f32>) -> vec3<f32> {
    let cutoff = step(srgb, vec3(0.04045));
    let higher = pow((srgb + vec3<f32>(0.055))/vec3<f32>(1.055), vec3<f32>(2.4));
    let lower = srgb/vec3(12.92);

    return mix(higher, lower, cutoff);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var dimensions = textureDimensions(texture);
    var eps = 0.0001;
    var half_pixel_width = 0.5 / f32(dimensions.x);

    // x_pos represents index of a column(pixel) on the output texture
    // - dimensions.x represents half of the output width, so we need multiply * 2
    // - input.tex_coords represents middle of pixel, so to shift to column index we need to shift by that value
    // - adding eps to avoid numerical errors when converting f32 -> u32
    var x_pos = u32((input.tex_coords.x * f32(dimensions.x) - half_pixel_width + eps) * 2.0);
    // x_pos/2 is calculated before conversion to float to make sure that reminder is lost for odd column.
    var tex_coords = vec2((f32(x_pos/2) / f32(dimensions.x)) + half_pixel_width, input.tex_coords.y);

    var uyvy = textureSample(texture, sampler_, tex_coords);

    var u = uyvy.x;
    var v = uyvy.z;
    var y = uyvy.y;
    if (x_pos % 2 != 0) {
        y = uyvy.w;
    }

    let r = y + 1.40200 * (v - 128.0 / 255.0);
    let g = y - 0.34414 * (u - 128.0 / 255.0) - 0.71414 * (v - 128.0 / 255.0);
    let b = y + 1.77200 * (u - 128.0 / 255.0);

    let srgb = vec3<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0));
    return vec4(srgb_to_linear(srgb), 1.0);
}
