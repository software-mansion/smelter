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

fn linear_to_srgb(linear: vec3<f32>) -> vec3<f32> {
    let cutoff = step(linear, vec3<f32>(0.0031308));
    let higher = vec3<f32>(1.055)*pow(linear, vec3<f32>(1.0/2.4)) - vec3<f32>(0.055);
    let lower = linear * vec3<f32>(12.92);

    return mix(higher, lower, cutoff);
}

fn srgb_to_linear(srgb: vec3<f32>) -> vec3<f32> {
    let cutoff = step(srgb, vec3(0.04045));
    let higher = pow((srgb + vec3<f32>(0.055))/vec3<f32>(1.055), vec3<f32>(2.4));
    let lower = srgb/vec3(12.92);

    return mix(higher, lower, cutoff);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let original_linear = textureSample(texture, sampler_, input.tex_coords);
    let original_srgb = linear_to_srgb(original_linear.rgb);
    let a = max(original_linear.a, 0.0000);
    let corrected_rgb = vec3<f32>(
        clamp(original_srgb.r/a, 0.0, 1.0),
        clamp(original_srgb.g/a, 0.0, 1.0),
        clamp(original_srgb.b/a, 0.0, 1.0)
    );

    return vec4<f32>(
        srgb_to_linear(corrected_rgb),
        clamp(original_linear.a, 0.0, 1.0)
    );
}
