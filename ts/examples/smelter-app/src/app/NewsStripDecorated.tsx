import { Shader } from '@swmansion/smelter';
import React from 'react';

export type NewsStripDecoratedProps = {
  resolution: { width: number; height: number };
  opacity?: number;
  amplitudePx?: number;
  wavelengthPx?: number;
  speed?: number;
  phase?: number;
  removeColorTolerance?: number;
  removeColorEnabled?: boolean;
  children?: React.ReactElement;
};

export function NewsStripDecorated({
  resolution,
  opacity = 0.8,
  amplitudePx = 20,
  wavelengthPx = 800,
  speed = 0,
  phase = 0,
  removeColorTolerance = 0.4,
  removeColorEnabled = true,
  children,
}: NewsStripDecoratedProps) {
  const content = (
    <Shader
      shaderId="opacity"
      resolution={resolution}
      shaderParam={{
        type: 'struct',
        value: [{ type: 'f32', fieldName: 'opacity', value: opacity }],
      }}>
      {children}
    </Shader>
  );

  const waved = (
    <Shader
      shaderId="sine-wave"
      resolution={resolution}
      shaderParam={{
        type: 'struct',
        value: [
          { type: 'f32', fieldName: 'amplitude_px', value: amplitudePx },
          { type: 'f32', fieldName: 'wavelength_px', value: wavelengthPx },
          { type: 'f32', fieldName: 'speed', value: speed },
          { type: 'f32', fieldName: 'phase', value: phase },
        ],
      }}>
      {content}
    </Shader>
  );

  if (!removeColorEnabled) {
    return waved;
  }

  return (
    <Shader
      shaderId="remove-color"
      resolution={resolution}
      shaderParam={{
        type: 'struct',
        value: [
          { type: 'f32', fieldName: 'target_r', value: 1 },
          { type: 'f32', fieldName: 'target_g', value: 1 },
          { type: 'f32', fieldName: 'target_b', value: 1 },
          { type: 'f32', fieldName: 'tolerance', value: removeColorTolerance },
        ],
      }}>
      {waved}
    </Shader>
  );
}
