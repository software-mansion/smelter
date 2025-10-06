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

struct ShaderOptions {
    glyph_size: f32,
    gamma_correction: f32,
}

@group(0) @binding(0) var textures: binding_array<texture_2d<f32>, 16>;
@group(2) @binding(0) var sampler_: sampler;
@group(1) @binding(0) var<uniform> shader_options: ShaderOptions;

var<push_constant> base_params: BaseShaderParameters;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    output.position = vec4<f32>(input.position, 1.0);
    output.tex_coords = input.tex_coords;

    return output;
}

fn is_character(bitmap: u32, pos: vec2<f32>) -> f32 {
    // Transform texture coordinates to character bitmap coordinates (5 x 5 grid).
    let mapped_pos = floor(2.5 + (pos * vec2(-4.0, 4.0)));

    // Check if position is out of bounds.
    if (mapped_pos.x < 0.0 || mapped_pos.x > 4.0 || mapped_pos.y < 0.0 || mapped_pos.y > 4.0) {
        return 0.0;
    }

    // Convert 2D bitmap position to 1D bit index (row-major order)
    let bitmap_pos = u32(mapped_pos.x + 5.0 * mapped_pos.y);

    // Extract the bit at position 'a' from the character bitmap 'n'.
    if (((bitmap >> bitmap_pos) & 1u) != 1u) {
        return 0.0;
    }

    return 1.0;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (base_params.texture_count != 1u) {
        return vec4(0.0, 0.0, 0.0, 0.0);
    }

    let f32_output_resolution = vec2(f32(base_params.output_resolution.x), f32(base_params.output_resolution.y));

    let glyph_size = shader_options.glyph_size;
    let half_size = glyph_size * 0.5;

    // Pixel position on the texture.
    let pix = input.tex_coords * f32_output_resolution;

    // Sample color at pix position.
    let color_sample_pos = (floor(pix / glyph_size) * glyph_size) / f32_output_resolution;
    let color = textureSample(textures[0], sampler_, color_sample_pos);

    // Select char to render.
    let gray = pow(0.3 * color.r + 0.59 * color.g + 0.11 * color.b, shader_options.gamma_correction);
    var bitmap: u32 = 4096;

    if (gray > 0.2) { bitmap = 65600; }
    if (gray > 0.3) { bitmap = 163153; }
    if (gray > 0.4) { bitmap = 15255086; }
    if (gray > 0.5) { bitmap = 13121101; }
    if (gray > 0.6) { bitmap = 15252014; }
    if (gray > 0.7) { bitmap = 13195790; }
    if (gray > 0.8) { bitmap = 11512810; }

    // (-1, -1) position on the current char bitmap.
    let pos = vec2((pix.x / half_size) % 2 - 1, (pix.y / half_size) % 2 - 1);

    return vec4(color.rgb * is_character(bitmap, pos), 1.0);
}
