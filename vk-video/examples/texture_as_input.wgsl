struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

const VERTICES: array<vec3<f32>, 3> = array<vec3<f32>, 3>(
    vec3<f32>(-0.5, 0.0, 0.0),
    vec3<f32>(0.5, 0.0, 0.0),
    vec3<f32>(0.0, 1.0, 0.0),
);

const COLORS: array<vec3<f32>, 3> = array<vec3<f32>, 3>(
    vec3<f32>(1.0, 0.0, 0.0),
    vec3<f32>(0.0, 1.0, 0.0),
    vec3<f32>(0.0, 0.0, 1.0),
);

var<immediate> time: f32;

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    var output: VertexOutput;

    output.position = vec4(VERTICES[idx], 1.0);
    output.color = COLORS[idx];

    output.position.x += sin(time * 0.125 * 3.1416);

    return output;
}

@fragment
fn fs_main_y(input: VertexOutput) -> @location(0) f32 {
    let conversion_weights = vec3<f32>(0.2126, 0.7152, 0.0722);
    return clamp(dot(input.color, conversion_weights), 0.0, 1.0);
}

@fragment
fn fs_main_uv(input: VertexOutput) -> @location(0) vec2<f32> {
    let conversion_weights = mat3x2<f32>(
        -0.1146,  0.5,
        -0.3854, -0.4542,
         0.5,    -0.0458,
    );
    let conversion_bias = vec2<f32>(0.5, 0.5);

    return clamp(conversion_weights * input.color + conversion_bias, vec2(0.0, 0.0), vec2(1.0, 1.0));
}


