/// Depends on uniform params (1st bind group)

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

struct BaseShaderParameters {
    plane_id: i32,
    time: f32,
    output_resolution: vec2<u32>,
    texture_count: u32,
}

struct CircleLayout {
    left_px: u32,
    top_px: u32,
    width_px: u32,
    height_px: u32,
    background_color: vec4<f32>, // 0.0 - 1.0 range
}

@group(0) @binding(0) var textures: binding_array<texture_2d<f32>, 16>;
@group(1) @binding(0) var<uniform> circle_layouts: array<CircleLayout, 4>;
@group(2) @binding(0) var sampler_: sampler;

var<push_constant> base_params: BaseShaderParameters;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let circle_layout: CircleLayout = circle_layouts[base_params.plane_id];

    let x_scale: f32 = f32(circle_layout.width_px) / f32(base_params.output_resolution.x);
    let y_scale: f32 = f32(circle_layout.height_px) / f32(base_params.output_resolution.y);

    /// Location of scaled center of layout after transition in clip space coords.
    /// "circle_layout.left_px + circle_layout.width_px) / 2.0" -> the middle of layout in pixel coords
    /// "/ base_params.output_resolution.x" -> scaled to [0, 1.0] coords
    /// "* 2.0 - 1.0" -> scaled to clip space coords ([-1.0, 1.0] range)
    let center_x: f32 = ((f32(circle_layout.left_px) + (f32(circle_layout.width_px) / 2.0)) / f32(base_params.output_resolution.x)) * 2.0 - 1.0;
    let center_y: f32 = 1.0 - ((f32(circle_layout.top_px) + (f32(circle_layout.height_px) / 2.0)) / f32(base_params.output_resolution.y)) * 2.0;

    let output_x = input.position.x * x_scale + center_x;
    let output_y = input.position.y * y_scale + center_y;

    output.position = vec4(output_x, output_y, input.position.z, 1.0);
    output.tex_coords = input.tex_coords;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let circle_layout: CircleLayout = circle_layouts[base_params.plane_id];

    let center = vec2(0.5, 0.5);
    let uv = input.tex_coords - center;

    let len = length(uv);
    let is_in_circle: f32 = f32(len < 0.5);

    let sample = textureSample(textures[base_params.plane_id], sampler_, input.tex_coords);

    return sample * is_in_circle + circle_layout.background_color * (1.0 - is_in_circle);
}

