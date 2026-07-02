struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

struct TexParams {
    // Plane size / visible size. Coordinates above 1.0 clamp to the last
    // visible texel, so coded-alignment padding is edge-replicated.
    scale: vec2<f32>,
    // One source texel in texture coordinates.
    step: vec2<f32>,
}

@group(0) @binding(2) var<uniform> params: TexParams;

const VERTICES: array<vec3<f32>, 3> = array<vec3<f32>, 3>(
    vec3<f32>(-3.0, 1.0, 0.0),
    vec3<f32>(1.0, -3.0, 0.0),
    vec3<f32>(1.0, 1.0, 0.0),
);

const TEXTURE_COORDS: array<vec2<f32>, 3> = array<vec2<f32>, 3>(
    vec2<f32>(-1.0, 0.0),
    vec2<f32>(1.0, 2.0),
    vec2<f32>(1.0, 0.0),
);

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    var output: VertexOutput;

    output.position = vec4(VERTICES[idx], 1.0);
    output.tex_coords = TEXTURE_COORDS[idx] * params.scale;

    return output;
}

@group(0) @binding(0) var texture: texture_2d<f32>;
@group(0) @binding(1) var sampler_: sampler;

// Limited-range BT.709 RGB -> YCbCr:
// https://en.wikipedia.org/w/index.php?title=YCbCr&section=8#ITU-R_BT.709_conversion

fn luma(coords: vec2<f32>) -> f32 {
    let color = textureSample(texture, sampler_, coords);
    let y = color.r * 0.2126 + color.g * 0.7152 + color.b * 0.0722;
    // (235 - 16) / 255 footroom + headroom
    return clamp((y * 0.85882352941) + (16.0 / 255.0), 0.0, 1.0);
}

fn chroma(coords: vec2<f32>) -> vec2<f32> {
    let color = textureSample(texture, sampler_, coords);
    let u = color.r * -0.1146 + color.g * -0.3854 + color.b * 0.5;
    let v = color.r * 0.5 + color.g * -0.4542 + color.b * -0.0458;
    // (240 - 16) / 255 footroom + headroom
    return clamp(vec2(
        ((u + 0.5) * 0.87843137254) + (16.0 / 255.0),
        ((v + 0.5) * 0.87843137254) + (16.0 / 255.0),
    ), vec2(0.0), vec2(1.0));
}

// The planes are written as quarter-width RGBA8 reinterpretations of the
// byte layout: one fragment packs four Y samples or two UV pairs, which
// quarters the fragment count of the full-rate R8/RG8 form.

@fragment
fn fs_main_y(input: VertexOutput) -> @location(0) vec4<f32> {
    let d = vec2(params.step.x, 0.0);
    return vec4(
        luma(input.tex_coords - 1.5 * d),
        luma(input.tex_coords - 0.5 * d),
        luma(input.tex_coords + 0.5 * d),
        luma(input.tex_coords + 1.5 * d),
    );
}

@fragment
fn fs_main_uv(input: VertexOutput) -> @location(0) vec4<f32> {
    let d = vec2(params.step.x, 0.0);
    let left = chroma(input.tex_coords - d);
    let right = chroma(input.tex_coords + d);
    return vec4(left, right);
}
