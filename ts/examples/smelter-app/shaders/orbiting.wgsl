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
    opacity: f32,
    sprite_scale: f32,
    orbit_radius: f32,
    orbit_speed: f32,

    copies_f32: f32,
    colorize_amount: f32,

    sun_rays: f32,
    sun_anim_speed: f32,
    sun_base_radius: f32,
    sun_ray_amp: f32,
    sun_softness: f32,

};


@group(0) @binding(0)
var textures: binding_array<texture_2d<f32>, 16>;

@group(1) @binding(0)
var<uniform> shader_options: ShaderOptions;

@group(2) @binding(0)
var sampler_: sampler;

var<push_constant> base_params: BaseShaderParameters;

const PI : f32 = 3.1415926535;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(input.position, 1.0);
    out.tex_coords = input.tex_coords;
    return out;
}

fn sunMask(local_uv: vec2<f32>, time: f32) -> f32 {
    let p = local_uv - vec2<f32>(0.5, 0.5);
    let r = length(p);
    let angle = atan2(p.y, p.x);

    let rays = shader_options.sun_rays;
    let anim_speed = shader_options.sun_anim_speed;
    let base_radius = shader_options.sun_base_radius;
    let ray_amp = shader_options.sun_ray_amp;
    let softness = shader_options.sun_softness;

    let wave = sin(angle * rays + time * anim_speed);
    let radius = base_radius + wave * ray_amp;

    let m = 1.0 - smoothstep(radius, radius + softness, r);
    return clamp(m, 0.0, 1.0);
}


fn hueToRgb(h_in: f32) -> vec3<f32> {
    let h = fract(h_in);
    let h6 = h * 6.0;

    let r = clamp(abs(h6 - 3.0) - 1.0, 0.0, 1.0);
    let g = clamp(2.0 - abs(h6 - 2.0), 0.0, 1.0);
    let b = clamp(2.0 - abs(h6 - 4.0), 0.0, 1.0);

    return vec3<f32>(r, g, b);
}

fn sampleSprite(center_uv: vec2<f32>, uv: vec2<f32>, scale: f32, time: f32) -> vec4<f32> {
    let local = (uv - center_uv) / scale + vec2<f32>(0.5, 0.5);

    if (local.x < 0.0 || local.x > 1.0 || local.y < 0.0 || local.y > 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let mask = sunMask(local, time);
    if (mask <= 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let c = textureSample(textures[0], sampler_, local);
    return vec4<f32>(c.rgb * mask, c.a * mask);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (base_params.texture_count != 1u) {
        return vec4<f32>(0.0);
    }

    let uv = input.tex_coords;
    let op = clamp(shader_options.opacity, 0.0, 1.0);
    let time = base_params.time;
    let colorize = clamp(shader_options.colorize_amount, 0.0, 1.0);

    let copies_raw = shader_options.copies_f32;
    let copies_u32 = max(u32(floor(max(copies_raw, 0.0))), 1u);

    if (copies_u32 == 0u) {
        let c = textureSample(textures[0], sampler_, uv);
        return vec4<f32>(c.rgb * op, c.a * op);
    }

    let sprite_scale = shader_options.sprite_scale;
    let orbit_radius = shader_options.orbit_radius;
    let orbit_speed  = shader_options.orbit_speed;

    let center = vec2<f32>(0.5, 0.5);
    let base_angle = time * orbit_speed;

    var col = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    var i: u32 = 0u;
    loop {
        if (i >= copies_u32) { break; }

        let fi = f32(i);
        let fc = f32(copies_u32);

        let angle = base_angle + 2.0 * PI * (fi / fc);
        let offset = vec2<f32>(cos(angle), sin(angle)) * orbit_radius;
        let pos = center + offset;

        var s = sampleSprite(pos, uv, sprite_scale, time);

        let hue = fi / fc;
        let tint = hueToRgb(hue);

        let base_rgb = s.xyz;
        let tinted_rgb = base_rgb * tint;
        let final_rgb = base_rgb * (1.0 - colorize) + tinted_rgb * colorize;

        s = vec4<f32>(final_rgb, s.w);

        col = vec4<f32>(
            max(col.x, s.x),
            max(col.y, s.y),
            max(col.z, s.z),
            max(col.w, s.w),
        );

        i = i + 1u;
    }

    return vec4<f32>(col.xyz * op, col.w * op);
}
