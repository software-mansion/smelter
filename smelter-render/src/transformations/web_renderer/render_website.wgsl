struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

struct RenderInfo {
     is_website_texture: i32,
     transformation_matrix: mat4x4<f32>,
}

var<push_constant> render_info: RenderInfo;

@group(0) @binding(0) var texture: texture_2d<f32>;
@group(1) @binding(0) var sampler_: sampler;


@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    output.position = vec4(input.position, 1.0) * render_info.transformation_matrix;
    output.tex_coords = input.tex_coords;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sample = textureSample(texture, sampler_, input.tex_coords);

    // Website texture uses BGRA.
    // Convert BGRA to RGBA
    if render_info.is_website_texture == 1 {
        return vec4<f32>(sample.b, sample.g, sample.r, sample.a);
    }

    return sample;
}
