import { createElement, useContext, useEffect, useState, useSyncExternalStore } from 'react';
import { newBlockingTask } from '../hooks.js';
import { SmelterContext } from '../context/index.js';
import { inputRefIntoRawId, OfflineTimeContext } from '../internal.js';
import { InnerInputStream } from './InputStream.js';
import { newInternalStreamId } from '../context/internalStreamIdManager.js';
import type { ComponentBaseProps } from '../component.js';
import { useTimeLimitedComponent } from '../context/childrenLifetimeContext.js';
import type { RegisterMp4Input } from '../types/registerInput.js';

export type Mp4Props = Omit<ComponentBaseProps, 'children'> & {
  /**
   * Audio volume represented by a number between 0 and 1.
   */
  volume?: number;
  /**
   * Mute audio.
   */
  muted?: boolean;

  /**
   *  Url or path to the mp4 file. File path refers to the filesystem where Smelter server is deployed.
   */
  source: string;
};

function Mp4(props: Mp4Props) {
  const { muted, volume, ...otherProps } = props;
  const ctx = useContext(SmelterContext);
  const [inputId, setInputId] = useState(0);

  useEffect(() => {
    const newInputId = newInternalStreamId();
    setInputId(newInputId);
    const pathOrUrl: Pick<RegisterMp4Input, 'url' | 'serverPath'> =
      props.source.startsWith('http://') || props.source.startsWith('https://')
        ? { url: props.source }
        : { serverPath: props.source };
    let registerPromise: Promise<any>;

    const task = newBlockingTask(ctx);
    void (async () => {
      try {
        registerPromise = ctx.registerMp4Input(newInputId, {
          ...pathOrUrl,
          required: ctx.timeContext instanceof OfflineTimeContext,
          // offsetMs will be overridden by registerMp4Input implementation
        });
        await registerPromise;
      } finally {
        task.done();
      }
    })();
    return () => {
      task.done();
      void (async () => {
        await registerPromise.catch(() => {});
        await ctx.unregisterMp4Input(newInputId);
      })();
    };
  }, [props.source]);

  useInternalAudioInput(inputId, muted ? 0 : (volume ?? 1));
  useTimeLimitedMp4(inputId);

  return createElement(InnerInputStream, {
    ...otherProps,
    inputId: inputRefIntoRawId({
      type: 'output-specific-input',
      id: inputId,
      outputId: ctx.outputId,
    }),
  });
}

function useInternalAudioInput(inputId: number, volume: number) {
  const ctx = useContext(SmelterContext);
  useEffect(() => {
    if (inputId === 0) {
      return;
    }
    const options = { volume };
    ctx.audioContext.addInputAudioComponent(
      { type: 'output-specific-input', id: inputId, outputId: ctx.outputId },
      options
    );
    return () => {
      ctx.audioContext.removeInputAudioComponent(
        { type: 'output-specific-input', id: inputId, outputId: ctx.outputId },
        options
      );
    };
  }, [inputId, volume]);
}

function useTimeLimitedMp4(inputId: number) {
  const ctx = useContext(SmelterContext);
  const [startTime, setStartTime] = useState(0);
  useEffect(() => {
    setStartTime(ctx.timeContext.timestampMs());
  }, [inputId]);

  const internalStreams = useSyncExternalStore(
    ctx.internalInputStreamStore.subscribe,
    ctx.internalInputStreamStore.getSnapshot
  );
  const input = internalStreams[String(inputId)];
  useTimeLimitedComponent((input?.offsetMs ?? startTime) + (input?.videoDurationMs ?? 0));
  useTimeLimitedComponent((input?.offsetMs ?? startTime) + (input?.audioDurationMs ?? 0));
}

export default Mp4;
