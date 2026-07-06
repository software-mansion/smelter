struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4(input.position, 1.0);
    return output;
}

@group(0) @binding(0) var texture: texture_2d<f32>;

// Per-axis integer box reduction factor (power of two).
struct Factor {
    x: u32,
    y: u32,
}

var<immediate> factor: Factor;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let f = vec2<i32>(vec2<u32>(factor.x, factor.y));
    let base = vec2<i32>(input.position.xy) * f;
    let max_src = vec2<i32>(textureDimensions(texture)) - vec2<i32>(1, 1);

    var sum = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    for (var dy = 0; dy < f.y; dy++) {
        for (var dx = 0; dx < f.x; dx++) {
            let src = clamp(base + vec2<i32>(dx, dy), vec2<i32>(0, 0), max_src);
            sum += textureLoad(texture, src, 0);
        }
    }
    return sum / f32(f.x * f.y);
}
