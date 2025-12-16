struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

struct BaseShaderParameters {
    plane_id: i32,
    time: f32,
    output_resolution: vec2<u32>,
    texture_count: u32,
};

struct ShaderOptions {
    shadow_r: f32,
    shadow_g: f32,
    shadow_b: f32,
    opacity: f32,
    offset_x_px: f32,
    offset_y_px: f32,
    blur_px: f32,
    anim_amp_px: f32,
    anim_speed: f32,
};

@group(0) @binding(0) var textures: binding_array<texture_2d<f32>, 16>;
@group(1) @binding(0) var<uniform> shader_options: ShaderOptions;
@group(2) @binding(0) var sampler_: sampler;

var<push_constant> base_params: BaseShaderParameters;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4(input.position, 1.0);
    out.tex_coords = input.tex_coords;
    return out;
}

fn sample_alpha(uv: vec2<f32>) -> f32 {
    let c = textureSample(textures[0], sampler_, uv);
    return clamp(c.a, 0.0, 1.0);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (base_params.texture_count != 1u) { return vec4(0.0); }

    let res = vec2<f32>(f32(base_params.output_resolution.x), f32(base_params.output_resolution.y));

    let uv = input.tex_coords;
    let src = textureSample(textures[0], sampler_, uv);
    let src_a = clamp(src.a, 0.0, 1.0);

    let shadow_col = vec3<f32>(shader_options.shadow_r, shader_options.shadow_g, shader_options.shadow_b);
    let opacity = clamp(shader_options.opacity, 0.0, 1.0);
    let blur_px = max(0.0, shader_options.blur_px);
    let base_off_px = vec2<f32>(shader_options.offset_x_px, shader_options.offset_y_px);

    let t = base_params.time;
    let amp = shader_options.anim_amp_px;
    let w = shader_options.anim_speed;
    let anim_off_px = amp * vec2<f32>(sin(w * t), cos(w * t) * 0.5);

    let total_off_px = base_off_px + anim_off_px;
    let off_uv = total_off_px / res;

    let blur_uv = blur_px / res;
    var shadow_acc = 0.0;
    var weight_acc = 0.0;

    {
        let a = sample_alpha(clamp(uv + off_uv, vec2<f32>(0.0), vec2<f32>(1.0)));
        let w0 = 0.4;
        shadow_acc += a * w0;
        weight_acc += w0;
    }
    let ring = array<vec2<f32>, 8>(
        vec2<f32>( 1.0,  0.0), vec2<f32>(-1.0,  0.0),
        vec2<f32>( 0.0,  1.0), vec2<f32>( 0.0, -1.0),
        vec2<f32>( 0.7071,  0.7071), vec2<f32>(-0.7071,  0.7071),
        vec2<f32>( 0.7071, -0.7071), vec2<f32>(-0.7071, -0.7071)
    );
    for (var i: i32 = 0; i < 8; i = i + 1) {
        let dir = ring[i];
        let uv_s = clamp(uv + off_uv + dir * blur_uv, vec2<f32>(0.0), vec2<f32>(1.0));
        let a = sample_alpha(uv_s);
        let wi = 0.075;
        shadow_acc += a * wi;
        weight_acc += wi;
    }
    let shadow_alpha = (shadow_acc / max(0.0001, weight_acc)) * opacity;
    let inv_src = 1.0 - src_a;
    let out_rgb = shadow_col * shadow_alpha * inv_src + src.rgb;
    let out_a = clamp(src_a + shadow_alpha * inv_src, 0.0, 1.0);
    return vec4(out_rgb, out_a);
}


