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
    progress: f32,
    direction: f32,
    perspective: f32,
    shadow_strength: f32,
    back_tint: f32,
    back_tint_strength: f32,
};

@group(0) @binding(0) var textures: binding_array<texture_2d<f32>, 16>;
@group(1) @binding(0) var<uniform> shader_options: ShaderOptions;
@group(2) @binding(0) var sampler_: sampler;

var<push_constant> base_params: BaseShaderParameters;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(input.position, 1.0);
    out.tex_coords = input.tex_coords;
    return out;
}

fn rotate_y(v: vec3<f32>, a: f32) -> vec3<f32> {
    let cos_a = cos(a);
    let sin_a = sin(a);
    return vec3<f32>( cos_a*v.x + sin_a*v.z, v.y, -sin_a*v.x + cos_a*v.z );
}
fn saturate(x: f32) -> f32 { return clamp(x, 0.0, 1.0); }

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (base_params.texture_count < 1u) {
        return vec4<f32>(0.0);
    }

    var out_col = vec4<f32>(0.0);

    let uv = input.tex_coords;
    let screen_xy = uv * 2.0 - vec2<f32>(1.0);

    let persp = max(shader_options.perspective, 0.0);
    let cam_z = 1.5 + 2.0 * persp;
    let ro    = vec3<f32>(0.0, 0.0, cam_z);
    let rd    = normalize(vec3<f32>(screen_xy, -cam_z));

    let dir_val = clamp(shader_options.direction, -1.0, 1.0);
    let pivot_x = -dir_val;
    let pivot   = vec3<f32>(pivot_x, 0.0, 0.0);

    let t     = clamp(shader_options.progress, 0.0, 1.0);
    let angle = t * 3.1415926535;

    let n0 = vec3<f32>(0.0, 0.0, 1.0);
    let n  = rotate_y(n0, angle);
    let p0 = rotate_y(vec3<f32>(0.0) - pivot, angle) + pivot;

    let denom = dot(rd, n);
    if (abs(denom) > 1e-5) {
        let thit = dot(p0 - ro, n) / denom;
        if (thit > 0.0) {
            let hit   = ro + rd * thit;
            let local = rotate_y(hit - pivot, -angle) + pivot;

            if (abs(local.x) <= 1.0 && abs(local.y) <= 1.0) {
                var uv_page = local.xy * 0.5 + vec2<f32>(0.5, 0.5);

                let facing = dot(n, normalize(ro - hit));
                var page = vec4<f32>(0.0);

                if (facing > 0.0) {
                    page = textureSample(textures[0], sampler_, uv_page);
                } else {
                    var back = textureSample(textures[0], sampler_, vec2<f32>(1.0 - uv_page.x, uv_page.y));
                    let tinted = back.rgb * shader_options.back_tint;
                    var mixed_rgb = mix(back.rgb, tinted, clamp(shader_options.back_tint_strength, 0.0, 1.0));
                    back = vec4<f32>(mixed_rgb, back.a);
                    page = back;
                }

                let fold   = abs(local.x - pivot.x);
                let crease = 1.0 - clamp(fold * 0.7, 0.0, 1.0);
                let turn   = clamp(abs(sin(angle)), 0.0, 1.0);
                let shadow = 1.0 - shader_options.shadow_strength * crease * turn;

                let shaded_rgb = page.rgb * shadow;
                page = vec4<f32>(shaded_rgb, page.a);

                out_col = page;
            }
        }
    }

    return out_col;
}
