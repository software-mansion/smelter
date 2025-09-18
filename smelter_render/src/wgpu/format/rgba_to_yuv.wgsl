struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    output.position = vec4(input.position, 1.0);
    output.tex_coords = input.tex_coords;

    return output;
}

@group(0) @binding(0) var texture: texture_2d<f32>;
@group(1) @binding(0) var sampler_: sampler;

var<push_constant> plane_selector: u32;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) f32 {
    let color = textureSample(texture, sampler_, input.tex_coords);
    var component: f32;

    // YUV conversion from: https://en.wikipedia.org/w/index.php?title=YCbCr&section=8#ITU-R_BT.709_conversion
    // YUV values footroom needs to be added
    // UV planes are returned in range (-0.5, 0.5) and need to be moved to (0, 1)
    if(plane_selector == 0u) {
        // Y plane
        let y = color.r * 0.2126 + color.g * 0.7152 + color.b * 0.0722;
        // (235 - 16) / (255 - 0) = (219 / 255) ~= .858
        component = (y * 0.85882352941) + (16.0/255.0);
    } else if(plane_selector == 1u) {
        // U plane
        let u = color.r * -0.1146 + color.g * -0.3854 + color.b * 0.5;
        // (240 - 16) / (255 - 0) = (224 / 255) ~= .878
        component = ((u + 0.5) * 0.87843137254) + (16.0/255.0);
    } else if(plane_selector == 2u) {
        // V plane
        let v = color.r * 0.5 + color.g * -0.4542 + color.b * -0.0458;
        // (240 - 16) / (255 - 0) = (224 / 255) ~= .878
        component = ((v + 0.5) * 0.87843137254) + (16.0/255.0);
    } else {
        component = 0.0;
    }

    return clamp(component, 0.0, 1.0);
}
