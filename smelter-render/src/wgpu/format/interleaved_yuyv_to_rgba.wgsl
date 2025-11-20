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
    // x_pos/2 is calculated before conversion to float to make sure that remainder is lost for odd column.
    var tex_coords = vec2((f32(x_pos/2) / f32(dimensions.x)) + half_pixel_width, input.tex_coords.y);

    var yuyv = textureSample(texture, sampler_, tex_coords);

    var u = yuyv.y;
    var v = yuyv.w;
    var y = yuyv.x;
    if (x_pos % 2 != 0) {
        y = yuyv.z;
    }

    // YUV conversion from: https://en.wikipedia.org/w/index.php?title=YCbCr&section=8#ITU-R_BT.709_conversion
    // YUV values footroom needs to be removed
    // UV planes are in range (0, 1), but equation expects (-0.5, 0.5)

    // (235 - 16) / (255 - 0) = (219 / 255) ~= .858
    y = clamp((y - (16.0/255.0)) / 0.85882352941, 0.0, 1.0);
    // (240 - 16) / (255 - 0) = (224 / 255) ~= .878
    u = clamp((u - (16.0/255.0)) / 0.87843137254, 0.0, 1.0);
    v = clamp((v - (16.0/255.0)) / 0.87843137254, 0.0, 1.0);

    let r = y + 1.5748 * (v - 0.5);
    let g = y - 0.1873 * (u - 0.5) - 0.4681 * (v - 0.5);
    let b = y + 1.8556 * (u - 0.5);

    return vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);
}
