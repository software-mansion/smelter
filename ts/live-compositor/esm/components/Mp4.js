import { createElement, useContext, useEffect, useState, useSyncExternalStore } from 'react';
import { newBlockingTask } from '../hooks.js';
import { LiveCompositorContext } from '../context/index.js';
import { inputRefIntoRawId, OfflineTimeContext } from '../internal.js';
import { InnerInputStream } from './InputStream.js';
import { newInternalStreamId } from '../context/internalStreamIdManager.js';
import { useTimeLimitedComponent } from '../context/childrenLifetimeContext.js';
function Mp4(props) {
    const { muted, volume, ...otherProps } = props;
    const ctx = useContext(LiveCompositorContext);
    const [inputId, setInputId] = useState(0);
    useEffect(() => {
        const newInputId = newInternalStreamId();
        setInputId(newInputId);
        const task = newBlockingTask(ctx);
        const pathOrUrl = props.source.startsWith('http://') || props.source.startsWith('https://')
            ? { url: props.source }
            : { path: props.source };
        let registerPromise;
        void (async () => {
            try {
                registerPromise = ctx.registerMp4Input(newInputId, {
                    ...pathOrUrl,
                    required: ctx.timeContext instanceof OfflineTimeContext,
                    // offsetMs will be overridden by registerMp4Input implementation
                });
                await registerPromise;
            }
            finally {
                task.done();
            }
        })();
        return () => {
            task.done();
            void (async () => {
                await registerPromise.catch(() => { });
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
function useInternalAudioInput(inputId, volume) {
    const ctx = useContext(LiveCompositorContext);
    useEffect(() => {
        if (inputId === 0) {
            return;
        }
        const options = { volume };
        ctx.audioContext.addInputAudioComponent({ type: 'output-specific-input', id: inputId, outputId: ctx.outputId }, options);
        return () => {
            ctx.audioContext.removeInputAudioComponent({ type: 'output-specific-input', id: inputId, outputId: ctx.outputId }, options);
        };
    }, [inputId, volume]);
}
function useTimeLimitedMp4(inputId) {
    const ctx = useContext(LiveCompositorContext);
    const [startTime, setStartTime] = useState(0);
    useEffect(() => {
        setStartTime(ctx.timeContext.timestampMs());
    }, [inputId]);
    const internalStreams = useSyncExternalStore(ctx.internalInputStreamStore.subscribe, ctx.internalInputStreamStore.getSnapshot);
    const input = internalStreams[String(inputId)];
    useTimeLimitedComponent((input?.offsetMs ?? startTime) + (input?.videoDurationMs ?? 0));
    useTimeLimitedComponent((input?.offsetMs ?? startTime) + (input?.audioDurationMs ?? 0));
}
export default Mp4;
