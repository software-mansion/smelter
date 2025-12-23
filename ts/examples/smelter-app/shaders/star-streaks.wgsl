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

// Star streak overlay parameters
struct ShaderOptions {
    // Number of vertical streak columns per unit of the min(screenWidth, screenHeight)
    line_density: f32,       // [1 .. 200]
    // Horizontal thickness of each streak in pixels
    thickness_px: f32,       // [0.5 .. 10]
    // Downward speed in "min-dimension units per second"
    speed: f32,              // [0 .. 5]
    // Horizontal jitter amplitude in pixels for each column
    jitter_amp_px: f32,      // [0 .. 50]
    // Jitter oscillation frequency (Hz)
    jitter_freq: f32,        // [0 .. 10]
    // Repetition count of bright segments per min-dimension in Y. 0 => continuous streaks
    dash_repeat: f32,        // [0 .. 50]
    // Duty cycle of bright segment within each dash period [0..1]
    dash_duty: f32,          // [0 .. 1]
    // Brightness scale of the streaks
    brightness: f32,         // [0 .. 3]
}

@group(0) @binding(0) var textures: binding_array<texture_2d<f32>, 16>;
@group(1) @binding(0) var<uniform> shader_options: ShaderOptions;
@group(2) @binding(0) var sampler_: sampler;
var<push_constant> base_params: BaseShaderParameters;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 1.0);
    output.tex_coords = input.tex_coords;
    return output;
}

fn saturate1(x: f32) -> f32 { return clamp(x, 0.0, 1.0); }
fn saturate3(x: vec3<f32>) -> vec3<f32> { return clamp(x, vec3<f32>(0.0), vec3<f32>(1.0)); }

fn hash11(p: f32) -> f32 {
    // Simple float hash: fract(sin(p)*C)
    let s = sin(p * 127.1 + 311.7);
    return fract(s * 43758.5453);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (base_params.texture_count < 1u) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    let uv = input.tex_coords;
    let resolution = vec2<f32>(f32(base_params.output_resolution.x), f32(base_params.output_resolution.y));

    let min_dim = min(resolution.x, resolution.y);
    let center = vec2<f32>(0.5, 0.5);
    let s = (uv - center) * vec2<f32>(resolution.x / min_dim, resolution.y / min_dim); // "s-space"

    // Sample base scene
    let base_col = textureSample(textures[0], sampler_, uv);

    // Parameters
    let line_density = max(1.0, shader_options.line_density);
    let thickness_s = max(0.0005, shader_options.thickness_px / min_dim);
    let speed = max(0.0, shader_options.speed);
    let jitter_s = shader_options.jitter_amp_px / min_dim;
    let jfreq = shader_options.jitter_freq;
    let dash_repeat = max(0.0, shader_options.dash_repeat);
    let dash_duty = clamp(shader_options.dash_duty, 0.0, 1.0);
    let bright = max(0.0, shader_options.brightness);

    // Column and local X coordinate
    let cell_w = 1.0 / line_density;          // width of one column in s-space
    let x_cell = s.x / cell_w;                 // continuous column coordinate
    let cidx = floor(x_cell);                  // integer column index
    let fracx = fract(x_cell) - 0.5;           // [-0.5, 0.5)

    // Column-dependent jitter (horizontal)
    let phase = hash11(cidx + 17.0);
    let jitter = (hash11(cidx * 1.37 + 2.1) - 0.5) * 2.0 * jitter_s * sin(6.2831853 * (jfreq * base_params.time + phase));
    let dx = abs(fracx * cell_w + jitter);     // horizontal distance in s-units
    let mask_x = 1.0 - smoothstep(0.0, thickness_s, dx); // 1 near center line, 0 outside thickness

    // Optional vertical dashing to create moving bright segments
    var mask_y: f32;
    if (dash_repeat <= 0.0001) {
        mask_y = 1.0;
    } else {
        let dash_phase = fract(s.y * dash_repeat + base_params.time * speed + hash11(cidx * 3.11));
        let d = abs(dash_phase - 0.5); // 0 at center of the dash window
        let edge = max(0.0001, 0.5 * (1.0 - dash_duty));
        // Linear falloff to 0 outside the duty window
        mask_y = saturate1((edge - d) / edge);
    }

    let streak = bright * mask_x * mask_y;
    let col = vec3<f32>(1.0); // white streaks
    let out_rgb = saturate3(base_col.rgb + col * streak);
    return vec4<f32>(out_rgb, base_col.a);
}


