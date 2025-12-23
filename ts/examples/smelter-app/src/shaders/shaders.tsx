import path from 'path';
import fs from 'fs';

type ShaderParam = {
  name: string;
  type: string;
  minValue?: number;
  maxValue?: number;
  defaultValue?: number;
};

type AvailableShader = {
  id: string;
  isActive: boolean;
  isVisible: boolean;
  name: string;
  description: string;
  shaderFile: string;
  params?: ShaderParam[];
};

export type PublicShader = AvailableShader & {
  iconSvg: string;
};

export type ShaderParamConfig = {
  paramName: string;
  paramValue: number;
};

export type ShaderConfig = {
  shaderName: string;
  shaderId: string;
  enabled: boolean;
  params: ShaderParamConfig[];
};

const AVAILABLE_SHADERS: AvailableShader[] = [
  {
    id: 'ascii-filter',
    isActive: true,
    isVisible: true,
    name: 'ASCII Filter',
    description: 'A filter that converts the input video to ASCII art.',
    shaderFile: 'ascii-filter.wgsl',
    params: [
      {
        name: 'glyph_size',
        type: 'number',
        minValue: 1,
        maxValue: 100,
        defaultValue: 10,
      },
      {
        name: 'gamma_correction',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.3,
      },
    ],
  },
  {
    id: 'grayscale',
    isActive: true,
    isVisible: true,
    name: 'Grayscale',
    description: 'A filter that converts the input video to grayscale.',
    shaderFile: 'grayscale.wgsl',
  },
  {
    id: 'opacity',
    isActive: true,
    isVisible: true,
    name: 'Opacity',
    description: 'A filter that sets the opacity of the input video.',
    shaderFile: 'opacity.wgsl',
    params: [
      {
        name: 'opacity',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 1,
      },
    ],
  },
  {
    id: 'page-flip-1',
    isActive: true,
    isVisible: false,
    name: 'Page Flip',
    description: 'A 3D page flip transition/filter with realistic shading and back tint option.',
    shaderFile: 'page-flip-1.wgsl',
    params: [
      {
        name: 'progress',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0,
      },
      {
        name: 'direction',
        type: 'number',
        minValue: -1,
        maxValue: 1,
        defaultValue: 1,
      },
      {
        name: 'perspective',
        type: 'number',
        minValue: 0,
        maxValue: 2,
        defaultValue: 1,
      },
      {
        name: 'shadow_strength',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.35,
      },
      {
        name: 'back_tint',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.95,
      },
      {
        name: 'back_tint_strength',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.33,
      },
    ],
  },
  {
    id: 'brightness-contrast',
    isActive: true,
    isVisible: true,
    name: 'Brightness & Contrast',
    description: 'A shader that adjusts the brightness and contrast of the input video.',
    shaderFile: 'brightness-contrast.wgsl',
    params: [
      {
        name: 'brightness',
        type: 'number',
        minValue: -1,
        maxValue: 1,
        defaultValue: 0,
      },
      {
        name: 'contrast',
        type: 'number',
        minValue: 0,
        maxValue: 10,
        defaultValue: 1,
      },
    ],
  },
  {
    id: 'circle-mask-outline',
    isActive: true,
    isVisible: true,
    name: 'Wrapped Outline',
    description:
      'Masks the image to a circle with a given diameter and adds an outline with configurable width and hue.',
    shaderFile: 'circle-mask-outline.wgsl',
    params: [
      {
        name: 'circle_diameter',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.83,
      },
      {
        name: 'outline_width',
        type: 'number',
        minValue: 0,
        maxValue: 0.1,
        defaultValue: 0.01,
      },
      {
        name: 'outline_hue',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.44,
      },
      {
        name: 'circle_scale',
        type: 'number',
        minValue: 0.1,
        maxValue: 3,
        defaultValue: 1.03,
      },
      {
        name: 'circle_offset_x_px',
        type: 'number',
        minValue: -3000,
        maxValue: 3000,
        defaultValue: 0,
      },
      {
        name: 'circle_offset_y_px',
        type: 'number',
        minValue: -3000,
        maxValue: 3000,
        defaultValue: 0,
      },
      {
        name: 'wobble_x_amp_px',
        type: 'number',
        minValue: 0,
        maxValue: 1000,
        defaultValue: 20,
      },
      {
        name: 'wobble_x_freq',
        type: 'number',
        minValue: 0,
        maxValue: 10,
        defaultValue: 2.1,
      },
      {
        name: 'wobble_y_amp_px',
        type: 'number',
        minValue: 0,
        maxValue: 1000,
        defaultValue: 20,
      },
      {
        name: 'wobble_y_freq',
        type: 'number',
        minValue: 0,
        maxValue: 10,
        defaultValue: 1,
      },
      {
        name: 'trail_enable',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 1,
      },
      {
        name: 'trail_spawn_interval',
        type: 'number',
        minValue: 0.01,
        maxValue: 5,
        defaultValue: 0.31,
      },
      {
        name: 'trail_speed',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.53,
      },
      {
        name: 'trail_shrink_speed',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.05,
      },
      {
        name: 'trail_x_amplitude',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.03,
      },
      {
        name: 'trail_x_frequency',
        type: 'number',
        minValue: 0,
        maxValue: 5,
        defaultValue: 1.2,
      },
      {
        name: 'trail_count_f32',
        type: 'number',
        minValue: 0,
        maxValue: 10,
        defaultValue: 10,
      },
      {
        name: 'trail_opacity',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.24,
      },
    ],
  },
  {
    id: 'remove-color',
    isActive: true,
    isVisible: false,
    name: 'Remove Color',
    description: 'Removes the exact target RGB color by making it fully transparent.',
    shaderFile: 'remove-color.wgsl',
    params: [
      {
        name: 'target_r',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0,
      },
      {
        name: 'target_g',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 1,
      },
      {
        name: 'target_b',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0,
      },
      {
        name: 'tolerance',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.1,
      },
    ],
  },
  {
    id: 'orbiting',
    isActive: true,
    isVisible: true,
    name: 'Orbiting',
    description: 'A shader that orbits the input video.',
    shaderFile: 'orbiting.wgsl',
    params: [
      {
        name: 'opacity',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 1,
      },
      {
        name: 'sprite_scale',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.5,
      },
      {
        name: 'orbit_radius',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.5,
      },
      {
        name: 'orbit_speed',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.5,
      },
      {
        name: 'copies_f32',
        type: 'number',
        minValue: 1,
        maxValue: 10,
        defaultValue: 3,
      },
      {
        name: 'colorize_amount',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0,
      },
      {
        name: 'sun_rays',
        type: 'number',
        minValue: 0,
        maxValue: 20,
        defaultValue: 10,
      },
      {
        name: 'sun_anim_speed',
        type: 'number',
        minValue: 0,
        maxValue: 20,
        defaultValue: 3,
      },
      {
        name: 'sun_base_radius',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.35,
      },
      {
        name: 'sun_ray_amp',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.09,
      },
      {
        name: 'sun_softness',
        type: 'number',
        minValue: 0,
        maxValue: 1,
        defaultValue: 0.06,
      },
    ],
  },
  {
    id: 'sine-wave',
    isActive: true,
    isVisible: false,
    name: 'Sine Wave Distortion',
    description:
      'Applies a vertical sine-wave distortion along X with configurable amplitude, wavelength, speed and phase.',
    shaderFile: 'sine-wave.wgsl',
    params: [
      {
        name: 'amplitude_px',
        type: 'number',
        minValue: 0,
        maxValue: 400,
        defaultValue: 20,
      },
      {
        name: 'wavelength_px',
        type: 'number',
        minValue: 10,
        maxValue: 4000,
        defaultValue: 800,
      },
      {
        name: 'speed',
        type: 'number',
        minValue: -20,
        maxValue: 20,
        defaultValue: 2.5,
      },
      {
        name: 'phase',
        type: 'number',
        minValue: -6.283,
        maxValue: 6.283,
        defaultValue: 0,
      },
    ],
  },
  {
    id: 'star-streaks',
    isActive: true,
    isVisible: true,
    name: 'Star Streaks',
    description:
      'Adds animated, parameterized white streaks (warp-star effect) over the input, simulating fast motion through space.',
    shaderFile: 'star-streaks.wgsl',
    params: [
      { name: 'line_density', type: 'number', minValue: 1, maxValue: 200, defaultValue: 40 },
      { name: 'thickness_px', type: 'number', minValue: 0.5, maxValue: 10, defaultValue: 2 },
      { name: 'speed', type: 'number', minValue: 0, maxValue: 5, defaultValue: 0.8 },
      { name: 'jitter_amp_px', type: 'number', minValue: 0, maxValue: 50, defaultValue: 4 },
      { name: 'jitter_freq', type: 'number', minValue: 0, maxValue: 10, defaultValue: 1.5 },
      { name: 'dash_repeat', type: 'number', minValue: 0, maxValue: 50, defaultValue: 10 },
      { name: 'dash_duty', type: 'number', minValue: 0.0, maxValue: 1.0, defaultValue: 0.4 },
      { name: 'brightness', type: 'number', minValue: 0.0, maxValue: 3.0, defaultValue: 0.5 },
    ],
  },
  {
    id: 'soft-shadow',
    isActive: true,
    isVisible: true,
    name: 'Soft Shadow',
    description:
      'Adds a soft, slightly animated shadow behind the input. Color, opacity, offset and blur are configurable.',
    shaderFile: 'soft-shadow.wgsl',
    params: [
      { name: 'shadow_r', type: 'number', minValue: 0, maxValue: 1, defaultValue: 0.0 },
      { name: 'shadow_g', type: 'number', minValue: 0, maxValue: 1, defaultValue: 0.0 },
      { name: 'shadow_b', type: 'number', minValue: 0, maxValue: 1, defaultValue: 0.0 },
      { name: 'opacity', type: 'number', minValue: 0, maxValue: 1, defaultValue: 0.5 },
      { name: 'offset_x_px', type: 'number', minValue: -200, maxValue: 200, defaultValue: 8 },
      { name: 'offset_y_px', type: 'number', minValue: -200, maxValue: 200, defaultValue: 8 },
      { name: 'blur_px', type: 'number', minValue: 0, maxValue: 200, defaultValue: 6 },
      { name: 'anim_amp_px', type: 'number', minValue: 0, maxValue: 30, defaultValue: 2 },
      { name: 'anim_speed', type: 'number', minValue: 0, maxValue: 20, defaultValue: 2 },
    ],
  },
];

class ShadersController {
  public get shaders(): PublicShader[] {
    const baseIconsDir = path.resolve(__dirname, '../../shaders/icons');
    return AVAILABLE_SHADERS.filter(shader => shader.isActive).map(shader => {
      const iconPath = path.join(baseIconsDir, `${shader.id}.svg`);
      let iconSvg = '';
      try {
        iconSvg = fs.readFileSync(iconPath, { encoding: 'utf-8' });
      } catch {
        iconSvg =
          '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><rect width="24" height="24" fill="#888"/></svg>';
      }
      return { ...shader, iconSvg };
    });
  }
}

const shadersController = new ShadersController();
export default shadersController;
