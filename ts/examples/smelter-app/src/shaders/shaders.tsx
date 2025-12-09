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
