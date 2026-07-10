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

struct Mapping {
    axis: u32,        // 0 = horizontal, 1 = vertical
    scale: f32,       // source texels per output texel
    offset: f32,      // crop offset along `axis`
    perp_offset: i32, // whole-texel crop offset on the other axis, applied 1:1
}

var<immediate> mapping: Mapping;

const PI: f32 = 3.14159265359;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let dim = vec2<i32>(textureDimensions(texture));
    let pos = vec2<i32>(input.position.xy);
    let out_coord = select(pos.x, pos.y, mapping.axis == 1u);
    let max_src = select(dim.x, dim.y, mapping.axis == 1u) - 1;
    let perp = clamp(
        select(pos.y, pos.x, mapping.axis == 1u) + mapping.perp_offset,
        0,
        select(dim.y, dim.x, mapping.axis == 1u) - 1,
    );

    // Kernel widens with the ratio to cover the full source footprint.
    let kernel_scale = max(mapping.scale, 1.0);
    let inv_k = 1.0 / kernel_scale;
    let support = 3.0 * kernel_scale;
    let center = mapping.offset + (f32(out_coord) + 0.5) * mapping.scale - 0.5;
    let first = ceil(center - support);
    let taps = i32(ceil(2.0 * support)) + 1;

    // lanczos3(x) = sinc(x) * sinc(x/3), sampled at a constant step: advance
    // sin(pi*x) and sin(pi*x/3) by rotation instead of a sin() per tap.
    let x0 = (first - center) * inv_k;
    var s1 = sin(PI * x0);
    var c1 = cos(PI * x0);
    var s3 = sin(PI * x0 / 3.0);
    var c3 = cos(PI * x0 / 3.0);
    let sd1 = sin(PI * inv_k);
    let cd1 = cos(PI * inv_k);
    let sd3 = sin(PI * inv_k / 3.0);
    let cd3 = cos(PI * inv_k / 3.0);

    var sum = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var weight_sum = 0.0;
    for (var t = 0; t < taps; t++) {
        let x = x0 + f32(t) * inv_k;
        var weight = 0.0;
        if abs(x) < 1e-5 {
            weight = 1.0;
        } else if abs(x) < 3.0 {
            weight = 3.0 * s1 * s3 / (PI * PI * x * x);
        }
        let src = clamp(i32(first) + t, 0, max_src);
        var coord = vec2<i32>(src, perp);
        if mapping.axis == 1u {
            coord = vec2<i32>(perp, src);
        }
        sum += textureLoad(texture, coord, 0) * weight;
        weight_sum += weight;

        let ns1 = s1 * cd1 + c1 * sd1;
        c1 = c1 * cd1 - s1 * sd1;
        s1 = ns1;
        let ns3 = s3 * cd3 + c3 * sd3;
        c3 = c3 * cd3 - s3 * sd3;
        s3 = ns3;
    }
    return sum / weight_sum;
}
