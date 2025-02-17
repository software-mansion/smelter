import type { _smelterInternals } from '@swmansion/smelter';
import type { SmelterEvent } from '../event';
import { SmelterEventType } from '../event';
import type Output from './output';

type LiveInputStreamStore<Id> = _smelterInternals.LiveInputStreamStore<Id>;

export function handleEvent(
  store: LiveInputStreamStore<string>,
  outputs: Record<string, Output>,
  event: SmelterEvent
) {
  if (event.type === SmelterEventType.VIDEO_INPUT_DELIVERED) {
    if (event.inputRef.type === 'global') {
      store.dispatchUpdate({
        type: 'update_input',
        input: { inputId: event.inputRef.id, videoState: 'ready' },
      });
    } else if (event.inputRef.type === 'output-specific-input') {
      outputs[event.inputRef.outputId]?.inputStreamStore().dispatchUpdate({
        type: 'update_input',
        input: { inputId: event.inputRef.id, videoState: 'ready' },
      });
    }
  } else if (event.type === SmelterEventType.VIDEO_INPUT_PLAYING) {
    if (event.inputRef.type === 'global') {
      store.dispatchUpdate({
        type: 'update_input',
        input: { inputId: event.inputRef.id, videoState: 'playing' },
      });
    } else if (event.inputRef.type === 'output-specific-input') {
      outputs[event.inputRef.outputId]?.inputStreamStore().dispatchUpdate({
        type: 'update_input',
        input: { inputId: event.inputRef.id, videoState: 'playing' },
      });
    }
  } else if (event.type === SmelterEventType.VIDEO_INPUT_EOS) {
    if (event.inputRef.type === 'global') {
      store.dispatchUpdate({
        type: 'update_input',
        input: { inputId: event.inputRef.id, videoState: 'finished' },
      });
    } else if (event.inputRef.type === 'output-specific-input') {
      outputs[event.inputRef.outputId]?.inputStreamStore().dispatchUpdate({
        type: 'update_input',
        input: { inputId: event.inputRef.id, videoState: 'finished' },
      });
    }
  } else if (event.type === SmelterEventType.AUDIO_INPUT_DELIVERED) {
    if (event.inputRef.type === 'global') {
      store.dispatchUpdate({
        type: 'update_input',
        input: { inputId: event.inputRef.id, audioState: 'ready' },
      });
    } else if (event.inputRef.type === 'output-specific-input') {
      outputs[event.inputRef.outputId]?.inputStreamStore().dispatchUpdate({
        type: 'update_input',
        input: { inputId: event.inputRef.id, audioState: 'ready' },
      });
    }
  } else if (event.type === SmelterEventType.AUDIO_INPUT_PLAYING) {
    if (event.inputRef.type === 'global') {
      store.dispatchUpdate({
        type: 'update_input',
        input: { inputId: event.inputRef.id, audioState: 'playing' },
      });
    } else if (event.inputRef.type === 'output-specific-input') {
      outputs[event.inputRef.outputId]?.inputStreamStore().dispatchUpdate({
        type: 'update_input',
        input: { inputId: event.inputRef.id, audioState: 'playing' },
      });
    }
  } else if (event.type === SmelterEventType.AUDIO_INPUT_EOS) {
    if (event.inputRef.type === 'global') {
      store.dispatchUpdate({
        type: 'update_input',
        input: { inputId: event.inputRef.id, audioState: 'finished' },
      });
    } else if (event.inputRef.type === 'output-specific-input') {
      outputs[event.inputRef.outputId]?.inputStreamStore().dispatchUpdate({
        type: 'update_input',
        input: { inputId: event.inputRef.id, audioState: 'finished' },
      });
    }
  }
}
