import type { InputConfig } from '../app/store';
import type { ShaderParamStructField } from '@swmansion/smelter';
import {
  Text,
  View,
  InputStream,
  Image,
  Rescaler,
  useInputStreams,
  Shader,
} from '@swmansion/smelter';

import type { ReactElement } from 'react';
import type { ShaderConfig, ShaderParamConfig } from '../shaders/shaders';

type Resolution = { width: number; height: number };

function wrapWithShaders(
  component: ReactElement,
  shaders: ShaderConfig[] | undefined,
  resolution: Resolution,
  index: number = 0
): ReactElement {
  if (!shaders || index >= shaders.length) {
    return component;
  }
  const shader = shaders[index];
  const shaderParams = Array.isArray(shader.params)
    ? shader.params.map(
        (param: ShaderParamConfig) =>
          ({
            type: 'f32',
            fieldName: param.paramName,
            value: param.paramValue,
          }) as ShaderParamStructField
      )
    : [];
  return (
    <Shader
      shaderId={shader.shaderId}
      resolution={resolution}
      shaderParam={
        shaderParams.length > 0
          ? {
              type: 'struct',
              value: shaderParams,
            }
          : undefined
      }>
      {wrapWithShaders(component, shaders, resolution, index + 1)}
    </Shader>
  );
}

export function Input({ input }: { input: InputConfig }) {
  const streams = useInputStreams();
  const streamState = streams[input.inputId]?.videoState ?? 'finished';
  const resolution = { width: 1920, height: 1080 };

  const inputComponent = (
    <Rescaler style={resolution}>
      <View style={{ ...resolution, direction: 'column' }}>
        {streamState === 'playing' ? (
          <Rescaler style={{ rescaleMode: 'fill' }}>
            <InputStream inputId={input.inputId} volume={input.volume} />
          </Rescaler>
        ) : streamState === 'ready' ? (
          <View style={{ padding: 300 }}>
            <Rescaler style={{ rescaleMode: 'fit' }}>
              <Image imageId="spinner" />
            </Rescaler>
          </View>
        ) : streamState === 'finished' ? (
          <View style={{ padding: 300 }}>
            <Rescaler style={{ rescaleMode: 'fit' }}>
              <Text style={{ fontSize: 600 }}>Stream offline</Text>
            </Rescaler>
          </View>
        ) : (
          <View />
        )}
        {input.showTitle !== false && (
          <View
            style={{
              backgroundColor: '#493880',
              height: 90,
              padding: 20,
              borderRadius: 0,
              direction: 'column',
              overflow: 'visible',
              bottom: 0,
              left: 0,
            }}>
            <Text style={{ fontSize: 40, color: 'white' }}>{input?.title}</Text>
            <View style={{ height: 10 }} />
            <Text style={{ fontSize: 25, color: 'white' }}>{input?.description}</Text>
          </View>
        )}
      </View>
    </Rescaler>
  );

  const activeShaders = input.shaders.filter(shader => shader.enabled);

  return wrapWithShaders(inputComponent, activeShaders, resolution, 0);
}

export function SmallInput({
  input,
  resolution = { width: 640, height: 360 },
}: {
  input: InputConfig;
  resolution?: Resolution;
}) {
  const activeShaders = input.shaders.filter(shader => shader.enabled);

  const smallInputComponent = (
    <View
      style={{
        width: resolution.width,
        height: resolution.height,
        direction: 'column',
        overflow: 'visible',
      }}>
      <Rescaler style={{ rescaleMode: 'fit' }}>
        <InputStream inputId={input.inputId} volume={input.volume} />
      </Rescaler>
      {input.showTitle !== false && (
        <View
          style={{
            backgroundColor: '#493880',
            height: 40,
            padding: 20,
            borderRadius: 0,
            direction: 'column',
            overflow: 'visible',
            bottom: 0,
            left: 0,
          }}>
          <Text style={{ fontSize: 30, color: 'white' }}>{input.title}</Text>
        </View>
      )}
    </View>
  );

  if (activeShaders.length) {
    return (
      <Rescaler>{wrapWithShaders(smallInputComponent, activeShaders, resolution, 0)}</Rescaler>
    );
  }
  return <Rescaler>{smallInputComponent}</Rescaler>;
}
