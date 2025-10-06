type ShaderParam = {
  name: string;
  type: string;
  minValue?: number;
  maxValue?: number;
  defaultValue?: number;
};

type AvailableShader = {
  id: string;
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
    name: 'Grayscale',
    description: 'A filter that converts the input video to grayscale.',
    shaderFile: 'grayscale.wgsl',
  },
];

class ShadersController {
  public get shaders(): AvailableShader[] {
    return AVAILABLE_SHADERS;
  }
}

const shadersController = new ShadersController();
export default shadersController;
