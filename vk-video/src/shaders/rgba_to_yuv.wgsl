struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

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
    output.tex_coords = TEXTURE_COORDS[idx];

    return output;
}

@group(0) @binding(0) var texture: texture_2d<f32>;
@group(1) @binding(0) var sampler_: sampler;

@fragment
fn fs_main_y(input: VertexOutput) -> @location(0) f32 {
    let color = textureSample(texture, sampler_, input.tex_coords).rgb;

    let conversion_weights = vec3<f32>(0.2126, 0.7152, 0.0722);

    return clamp(dot(color, conversion_weights), 0.0, 1.0);
}

@fragment
fn fs_main_uv(input: VertexOutput) -> @location(0) vec2<f32> {
    let color = textureSample(texture, sampler_, input.tex_coords).rgb;

    let conversion_weights = mat3x2<f32>(
        -0.1146,  0.5,
        -0.3854, -0.4542,
         0.5,    -0.0458,
    );
    let conversion_bias = vec2<f32>(0.5, 0.5);

    return clamp(conversion_weights * color + conversion_bias, vec2(0.0, 0.0), vec2(1.0, 1.0));
}


