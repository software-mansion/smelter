/// Layouts first 4 inputs in top left, top right, bottom left, bottom right quaters
/// and next ones in the center

/// Depends on base_params.plane_id

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@group(0) @binding(0) var textures: binding_array<texture_2d<f32>, 16>;
@group(2) @binding(0) var sampler_: sampler;

struct BaseShaderParameters {
    plane_id: i32,
    time: f32,
    output_resolution: vec2<u32>,
    texture_count: u32,
}

var<push_constant> base_params: BaseShaderParameters;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    let scaled_position = input.position.xy / 2.0;

    if (base_params.plane_id == -1) {
        output.position = vec4(input.position, 1.0);
    } else if (base_params.plane_id == 0) {
        output.position = vec4(scaled_position.x - 0.5, scaled_position.y + 0.5, input.position.z, 1.0);
    } else if (base_params.plane_id == 1) {
        output.position = vec4(scaled_position.x + 0.5, scaled_position.y + 0.5, input.position.z, 1.0);
    } else if (base_params.plane_id == 2) {
        output.position = vec4(scaled_position.x - 0.5, scaled_position.y - 0.5, input.position.z, 1.0);
    } else if (base_params.plane_id == 3) {
        output.position = vec4(scaled_position.x + 0.5, scaled_position.y - 0.5, input.position.z, 1.0);
    } else {
        output.position = vec4(scaled_position.x, scaled_position.y, input.position.z, 1.0);
    }

    output.tex_coords = input.tex_coords;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (base_params.plane_id == -1) {
        return vec4(1.0, 0.0, 0.0, 1.0);
    }

    return textureSample(textures[base_params.plane_id], sampler_, input.tex_coords);
}

