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
    // Fraction of the smaller output dimension (0..1)
    circle_diameter: f32,
    // Outline width as fraction of the smaller output dimension (0..1)
    outline_width: f32,
    // Hue in [0..1]
    outline_hue: f32,
    // Scale of the base circle and its content (uniform scale, >0)
    circle_scale: f32,
    // Horizontal offset of the base circle center in pixels (applied after scaling)
    circle_offset_x_px: f32,
    // Vertical offset of the base circle center in pixels (applied after scaling)
    circle_offset_y_px: f32,
    // Free oscillation amplitude in X (pixels)
    wobble_x_amp_px: f32,
    // Free oscillation frequency in X (Hz)
    wobble_x_freq: f32,
    // Free oscillation amplitude in Y (pixels)
    wobble_y_amp_px: f32,
    // Free oscillation frequency in Y (Hz)
    wobble_y_freq: f32,
    // Enable trail rings (0.0 disabled, >0 enabled)
    trail_enable: f32,
    // Seconds between spawns (>0 for visible effect)
    trail_spawn_interval: f32,
    // Upward speed in "min dimension" units per second
    trail_speed: f32,
    // Shrink speed (radius units per second, same units as outline_width)
    trail_shrink_speed: f32,
    // Horizontal wobble amplitude in "min dimension" units
    trail_x_amplitude: f32,
    // Horizontal wobble frequency in Hz
    trail_x_frequency: f32,
    // How many concurrent rings to show (clamped 0..10)
    trail_count_f32: f32,
    // Maximum opacity of rings (0..1)
    trail_opacity: f32,
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

fn hue_to_rgb(h: f32) -> vec3<f32> {
    // HSV with s=1, v=1
    let hh = fract(h) * 6.0;
    let i = i32(floor(hh)) % 6;
    let f = hh - floor(hh);
    let q = 1.0 - f; // when descending channel
    let t = f;       // when ascending channel

    switch i {
        case 0 { return vec3<f32>(1.0, t,   0.0); }
        case 1 { return vec3<f32>(q,   1.0, 0.0); }
        case 2 { return vec3<f32>(0.0, 1.0, t  ); }
        case 3 { return vec3<f32>(0.0, q,   1.0); }
        case 4 { return vec3<f32>(t,   0.0, 1.0); }
        default { return vec3<f32>(1.0, 0.0, q  ); }
    }
}

fn draw_ring(distance_s: f32, radius: f32, width: f32) -> f32 {
    // Symmetric ring: active where |d - radius| < width/2
    let half_w = width * 0.5;
    if (abs(distance_s - radius) < half_w) {
        return 1.0;
    }
    return 0.0;
}

// ---------------------- Helpers: geometry and space conversions ----------------------

fn compute_min_dim(res: vec2<f32>) -> f32 {
    return min(res.x, res.y);
}

fn to_scaled_space(uv: vec2<f32>, center: vec2<f32>, res: vec2<f32>) -> vec2<f32> {
    let min_dim = compute_min_dim(res);
    let scale = res / vec2<f32>(min_dim, min_dim);
    return (uv - center) * scale;
}

fn pixel_offset_to_s_space(res: vec2<f32>, offset_px: vec2<f32>) -> vec2<f32> {
    let min_dim = compute_min_dim(res);
    return offset_px / vec2<f32>(min_dim, min_dim);
}

fn wobble_delta_s(res: vec2<f32>, time: f32) -> vec2<f32> {
    let min_dim = compute_min_dim(res);
    let wobble_x = (shader_options.wobble_x_amp_px / min_dim) * sin(6.28318530718 * shader_options.wobble_x_freq * time);
    let wobble_y = (shader_options.wobble_y_amp_px / min_dim) * sin(6.28318530718 * shader_options.wobble_y_freq * time);
    return vec2<f32>(wobble_x, wobble_y);
}

fn shifted_s(uv: vec2<f32>, center: vec2<f32>, res: vec2<f32>, time: f32) -> vec2<f32> {
    let s = to_scaled_space(uv, center, res);
    let offset_s = pixel_offset_to_s_space(res, vec2<f32>(shader_options.circle_offset_x_px, shader_options.circle_offset_y_px));
    let wobble_s = wobble_delta_s(res, time);
    return s - (offset_s + wobble_s);
}

// ---------------------- Helpers: circle metrics and sampling ----------------------

fn base_radius() -> f32 {
    let cd = clamp(shader_options.circle_diameter, 0.0, 1.0);
    let circle_scale = max(shader_options.circle_scale, 0.001);
    return 0.5 * cd * circle_scale;
}

fn sample_inside_circle(uv: vec2<f32>, center: vec2<f32>, res: vec2<f32>, time: f32) -> vec4<f32> {
    // Convert the pixel offsets (including wobble) back to UV space
    let delta_uv_x = (shader_options.circle_offset_x_px / res.x)
        + (shader_options.wobble_x_amp_px / res.x) * sin(6.28318530718 * shader_options.wobble_x_freq * time);
    let delta_uv_y = (shader_options.circle_offset_y_px / res.y)
        + (shader_options.wobble_y_amp_px / res.y) * sin(6.28318530718 * shader_options.wobble_y_freq * time);
    let uv_scaled = center + (uv - center - vec2<f32>(delta_uv_x, delta_uv_y)) / max(shader_options.circle_scale, 0.001);
    return textureSample(textures[0], sampler_, uv_scaled);
}

// ---------------------- Helpers: trail rings and background ----------------------

fn accumulate_trail_rings(
    s_shifted: vec2<f32>,
    radius: f32,
    outline_w: f32,
    time: f32,
    outline_col: vec3<f32>
) -> vec4<f32> {
    var bg_color = vec4<f32>(0.0);
    if (shader_options.trail_enable <= 0.0) {
        return bg_color;
    }

        let spawn_interval = max(shader_options.trail_spawn_interval, 0.001);
        let speed = max(shader_options.trail_speed, 0.0);
        let shrink = max(shader_options.trail_shrink_speed, 0.0);
        let rings_f32 = clamp(shader_options.trail_count_f32, 0.0, 10.0);
        let rings = i32(floor(rings_f32 + 0.5));
        let max_alpha = clamp(shader_options.trail_opacity, 0.0, 1.0);

    if (rings <= 0 || shrink <= 0.0) {
        return bg_color;
    }

        let s = s_shifted;
    let age0 = time - floor(time / spawn_interval) * spawn_interval; // t % spawn_interval
            let life_time = radius / shrink;

            for (var i = 0; i < rings; i = i + 1) {
                let age = age0 + f32(i) * spawn_interval;
                if (age > life_time) {
                    continue;
                }
                let r_i = max(radius - shrink * age, 0.0);
        // Horizontal wobble for ring centers as they travel upward
                let x_amp = max(shader_options.trail_x_amplitude, 0.0);
                let x_freq = max(shader_options.trail_x_frequency, 0.0);
                let x_off = x_amp * sin(6.28318530718 * x_freq * age);
                let offset_s = vec2<f32>(x_off, -speed * age);
                let d_i = length(s - offset_s);

                let ring_mask = draw_ring(d_i, r_i, outline_w);
                if (ring_mask > 0.0) {
                    // Fade out over lifetime
                    let fade = 1.0 - clamp(age / life_time, 0.0, 1.0);
                    let a = max_alpha * fade;
                    let ring_col = vec4<f32>(outline_col, a);
                    // Over composite: ring over bg_color (background layer)
                    bg_color = ring_col + bg_color * (1.0 - ring_col.a);
                }
            }
    return bg_color;
}

// ---------------------- Main fragment routine ----------------------

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (base_params.texture_count != 1u) {
        return vec4(0.0, 0.0, 0.0, 0.0);
    }

    let uv = input.tex_coords;
    let res = vec2<f32>(f32(base_params.output_resolution.x), f32(base_params.output_resolution.y));
    let min_dim = min(res.x, res.y);

    // Scale UV differences so distance is computed in "min dimension" units -> circles remain circles.
    let center = vec2<f32>(0.5, 0.5);
    let t = base_params.time;
    let s_shifted = shifted_s(uv, center, res, t);
    let d = length(s_shifted);

    let radius = base_radius();
    let outline_w = clamp(shader_options.outline_width, 0.0, 1.0);
    let outline_col = hue_to_rgb(shader_options.outline_hue);

    // First, background composed of animated trail rings (they will be "under" the base circle)
    var bg_color = accumulate_trail_rings(s_shifted, radius, outline_w, t, outline_col);

    // Now draw the base circle and outline over the background (occluding rings beneath)
    var out_color = bg_color;

    // Inside circle: sample input, scaled together with the circle and re-centered by the same Y offset
    if (d < radius) {
        out_color = sample_inside_circle(uv, center, res, t);
    }

    // Outline over everything beneath
    if (d >= radius && d < radius + outline_w) {
        out_color = vec4<f32>(outline_col, 1.0);
    }

    return out_color;
}


