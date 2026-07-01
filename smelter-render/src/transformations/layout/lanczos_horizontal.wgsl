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
@group(1) @binding(0) var<uniform> source_width: f32;
@group(2) @binding(0) var sampler_: sampler;

fn sinc(x: f32) -> f32 {
    if abs(x) < 1e-5 {
        return 1.0;
    }
    let pix = 3.14159265359 * x;
    return sin(pix) / pix;
}

fn lanczos3_weight(x: f32) -> f32 {
    let ax = abs(x);
    if ax >= 3.0 {
        return 0.0;
    }
    return sinc(x) * sinc(x / 3.0);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let dim = vec2<i32>(textureDimensions(texture));
    let fc_x = source_width * input.tex_coords.x - 0.5;
    let center_x = floor(fc_x);
    let y = clamp(i32(floor(input.tex_coords.y * f32(dim.y))), 0, dim.y - 1);

    var sum = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var weight_sum = 0.0;
    for (var dx = -2; dx <= -1; dx++) {
        let sx = clamp(i32(center_x) + dx, 0, dim.x - 1);
        let weight = lanczos3_weight(fc_x - (center_x + f32(dx)));
        sum += textureLoad(texture, vec2<i32>(sx, y), 0) * weight;
        weight_sum += weight;
    }
    let w0 = lanczos3_weight(fc_x - center_x);
    let w1 = lanczos3_weight(fc_x - (center_x + 1.0));
    let combined_weight = w0 + w1;
    if abs(combined_weight) > 1e-6 {
        let t = clamp(w1 / combined_weight, 0.0, 1.0);
        let sx = clamp(center_x + 0.5 + t, 0.5, f32(dim.x) - 0.5);
        sum += textureSampleLevel(
            texture,
            sampler_,
            vec2<f32>(sx / f32(dim.x), (f32(y) + 0.5) / f32(dim.y)),
            0.0,
        ) * combined_weight;
        weight_sum += combined_weight;
    }
    for (var dx = 2; dx <= 3; dx++) {
        let sx = clamp(i32(center_x) + dx, 0, dim.x - 1);
        let weight = lanczos3_weight(fc_x - (center_x + f32(dx)));
        sum += textureLoad(texture, vec2<i32>(sx, y), 0) * weight;
        weight_sum += weight;
    }

    if weight_sum < 1e-6 {
        return textureLoad(texture, vec2<i32>(clamp(i32(center_x), 0, dim.x - 1), y), 0);
    }
    return sum / weight_sum;
}
