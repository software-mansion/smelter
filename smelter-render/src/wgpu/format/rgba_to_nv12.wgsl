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

const PI: f32 = 3.14159265359;

fn sinc(x: f32) -> f32 {
    if abs(x) < 1e-6 {
        return 1.0;
    }
    let px = PI * x;
    return sin(px) / px;
}

fn lanczos3_weight(x: f32) -> f32 {
    if abs(x) >= 3.0 {
        return 0.0;
    }
    return sinc(x) * sinc(x / 3.0);
}

fn sample_lanczos3_vertical(tex_coords: vec2<f32>) -> vec4<f32> {
    let dim = vec2<i32>(textureDimensions(texture));
    let x = clamp(i32(floor(tex_coords.x * f32(dim.x))), 0, dim.x - 1);
    let fc_y = f32(dim.y) * tex_coords.y - 0.5;
    let center_y = floor(fc_y);

    var sum = vec4<f32>(0.0);
    var weight_sum = 0.0;
    for (var dy = -2; dy <= -1; dy++) {
        let sy = clamp(i32(center_y) + dy, 0, dim.y - 1);
        let weight = lanczos3_weight(fc_y - (center_y + f32(dy)));
        sum += textureLoad(texture, vec2<i32>(x, sy), 0) * weight;
        weight_sum += weight;
    }
    let w0 = lanczos3_weight(fc_y - center_y);
    let w1 = lanczos3_weight(fc_y - (center_y + 1.0));
    let combined_weight = w0 + w1;
    if abs(combined_weight) > 1e-6 {
        let t = clamp(w1 / combined_weight, 0.0, 1.0);
        let sy = clamp(center_y + 0.5 + t, 0.5, f32(dim.y) - 0.5);
        sum += textureSampleLevel(
            texture,
            sampler_,
            vec2<f32>((f32(x) + 0.5) / f32(dim.x), sy / f32(dim.y)),
            0.0,
        ) * combined_weight;
        weight_sum += combined_weight;
    }
    for (var dy = 2; dy <= 3; dy++) {
        let sy = clamp(i32(center_y) + dy, 0, dim.y - 1);
        let weight = lanczos3_weight(fc_y - (center_y + f32(dy)));
        sum += textureLoad(texture, vec2<i32>(x, sy), 0) * weight;
        weight_sum += weight;
    }

    if weight_sum < 1e-6 {
        return textureLoad(texture, vec2<i32>(x, clamp(i32(center_y), 0, dim.y - 1)), 0);
    }
    return sum / weight_sum;
}

fn rgba_to_y(color: vec4<f32>) -> f32 {
    let y = color.r * 0.2126 + color.g * 0.7152 + color.b * 0.0722;
    return clamp((y * 0.85882352941) + (16.0/255.0), 0.0, 1.0);
}

fn rgba_to_uv(color: vec4<f32>) -> vec2<f32> {
    let u = color.r * -0.1146 + color.g * -0.3854 + color.b * 0.5;
    let v = color.r * 0.5 + color.g * -0.4542 + color.b * -0.0458;
    return clamp(vec2(
        ((u + 0.5) * 0.87843137254) + (16.0/255.0),
        ((v + 0.5) * 0.87843137254) + (16.0/255.0),
    ), vec2(0.0, 0.0), vec2(1.0, 1.0));
}

@fragment
fn fs_main_y(input: VertexOutput) -> @location(0) f32 {
    let color = textureSample(texture, sampler_, input.tex_coords);

    // YUV conversion from: https://en.wikipedia.org/w/index.php?title=YCbCr&section=8#ITU-R_BT.709_conversion
    // YUV values footroom needs to be added
    // Y plane
    return rgba_to_y(color);
}

@fragment
fn fs_main_uv(input: VertexOutput) -> @location(0) vec2<f32> {
    let color = textureSample(texture, sampler_, input.tex_coords);

    // YUV conversion from: https://en.wikipedia.org/w/index.php?title=YCbCr&section=8#ITU-R_BT.709_conversion
    // YUV values footroom needs to be added
    // UV planes are returned in range (-0.5, 0.5) and need to be moved to (0, 1)
    return rgba_to_uv(color);
}

@fragment
fn fs_lanczos_vertical_y(input: VertexOutput) -> @location(0) f32 {
    return rgba_to_y(sample_lanczos3_vertical(input.tex_coords));
}

@fragment
fn fs_lanczos_vertical_uv(input: VertexOutput) -> @location(0) vec2<f32> {
    return rgba_to_uv(sample_lanczos3_vertical(input.tex_coords));
}
