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
    let color = textureSample(texture, sampler_, input.tex_coords);
    let a = max(color.a, 0.00001);

    return vec4<f32>(
        clamp(color.r*a, 0.0, 1.0),
        clamp(color.g*a, 0.0, 1.0),
        clamp(color.b*a, 0.0, 1.0),
        clamp(color.a, 0.0, 1.0)
    );
}
