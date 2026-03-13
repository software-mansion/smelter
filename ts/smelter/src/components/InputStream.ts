import { createElement, useContext, useEffect, useState } from 'react';
import type * as Api from '../api.js';
import type { ComponentBaseProps, SceneComponent } from '../component.js';
import { createSmelterComponent } from '../component.js';
import { useAudioInput, useInputStreams } from '../hooks.js';
import { useTimeLimitedComponent } from '../context/childrenLifetimeContext.js';
import { SmelterContext } from '../context/index.js';
import { inputRefIntoRawId } from '../internal.js';

export type InputStreamProps = Omit<ComponentBaseProps, 'children'> & {
  /**
   * Id of an input. It identifies a stream registered using a `Smelter.registerInput`.
   */
  inputId: Api.InputId;
  /**
   * Audio volume represented by a number between 0 and 1.
   */
  volume?: number;
  /**
   * Mute audio.
   */
  muted?: boolean;
};

type AudioPropNames = 'muted' | 'volume';

export const InnerInputStream =
  createSmelterComponent<Omit<InputStreamProps, AudioPropNames>>(sceneBuilder);

function InputStream(props: InputStreamProps) {
  const { muted, volume, inputId, ...otherProps } = props;
  useAudioInput(inputId, {
    volume: muted ? 0 : (volume ?? 1),
  });
  useTimeLimitedInputStream(inputId);
  return createElement(InnerInputStream, {
    ...otherProps,
    inputId: inputRefIntoRawId({ type: 'global', id: inputId }),
  });
}

function useTimeLimitedInputStream(inputId: string) {
  const ctx = useContext(SmelterContext);

  // startTime is only needed for live case. In offline
  // mode offset is always set.
  const [startTime, setStartTime] = useState(0);
  useEffect(() => {
    setStartTime(ctx.timeContext.timestampMs());
  }, [inputId]);

  const inputs = useInputStreams();
  const input = inputs[inputId];
  const videoDurationMs = Math.max(0, (input?.videoDurationMs ?? 0) - (input?.seekMs ?? 0));
  const audioDurationMs = Math.max(0, (input?.audioDurationMs ?? 0) - (input?.seekMs ?? 0));
  useTimeLimitedComponent((input?.offsetMs ?? startTime) + videoDurationMs);
  useTimeLimitedComponent((input?.offsetMs ?? startTime) + audioDurationMs);
}

function sceneBuilder(props: InputStreamProps, _children: SceneComponent[]): Api.Component {
  return {
    type: 'input_stream',
    id: props.id,
    input_id: props.inputId,
  };
}

export default InputStream;
