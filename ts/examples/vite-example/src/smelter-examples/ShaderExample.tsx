import { Mp4, Shader } from '@swmansion/smelter';
import SmelterCanvasOutput from '../components/SmelterCanvasOutput';
import { useEffect } from 'react';
import { useSmelter } from '../hooks/useSmelter';

const EXAMPLE_SHADER = `
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

struct BaseShaderParameters {
    plane_id: i32,
    time: f32,
    output_resolution: vec2<u32>,
    texture_count: u32,
}

@group(0) @binding(0) var texture: texture_2d<f32>;
@group(2) @binding(0) var sampler_: sampler;

var<push_constant> base_params: BaseShaderParameters;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
  // Return transparent frame in case of different input video count
  if (base_params.texture_count != 1u) {
      return vec4(0.0, 0.0, 0.0, 0.0);
  }

  let pi = 3.14159;
  let effect_radius = abs(sin(base_params.time) / 2.0);
  let effect_angle = 2.0 * pi * abs(sin(base_params.time) / 2.0);

  let center = vec2(0.5, 0.5);
  let uv = input.tex_coords - center;

  let len = length(uv);
  let angle = atan2(uv.y, uv.x) + effect_angle * smoothstep(effect_radius, 0.0, len);
  let coords = vec2(len * cos(angle), len * sin(angle)) + center;

  return textureSample(texture, sampler_, coords);
}
`;

const MP4_URL =
  'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerEscapes.mp4';

function ShaderExample() {
  const smelter = useSmelter();
  useEffect(() => {
    if (!smelter) {
      return;
    }

    void smelter.registerShader('example_shader', {
      source: EXAMPLE_SHADER,
    });
  }, [smelter]);

  return (
    <div className="card">
      {smelter && (
        <SmelterCanvasOutput smelter={smelter} width={1280} height={720}>
          <Scene />
        </SmelterCanvasOutput>
      )}
    </div>
  );
}

function Scene() {
  return (
    <Shader shaderId="example_shader" resolution={{ width: 1280, height: 720 }}>
      <Mp4 source={MP4_URL} />
    </Shader>
  );
}

export default ShaderExample;
