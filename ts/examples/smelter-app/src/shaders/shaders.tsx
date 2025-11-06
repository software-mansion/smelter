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
  name: string;
  description: string;
  shaderFile: string;
  params?: ShaderParam[];
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
    name: 'Grayscale',
    description: 'A filter that converts the input video to grayscale.',
    shaderFile: 'grayscale.wgsl',
  },
  {
    id: 'opacity',
    isActive: true,
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
    isActive: false,
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
];

class ShadersController {
  public get shaders(): AvailableShader[] {
    return AVAILABLE_SHADERS.filter(shader => shader.isActive);
  }
}

const shadersController = new ShadersController();
export default shadersController;
