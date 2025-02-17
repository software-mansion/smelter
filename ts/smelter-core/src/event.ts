import { _smelterInternals } from '@swmansion/smelter';
import { parseInputRef } from './api/input';
import type { Logger } from 'pino';

export type SmelterEvent = _smelterInternals.SmelterEvent;
export const SmelterEventType = _smelterInternals.SmelterEventType;

export function parseEvent(event: any, logger: Logger): SmelterEvent | null {
  if (!event.type) {
    logger.error(`Malformed event: ${event}`);
    return null;
  } else if (
    [
      SmelterEventType.VIDEO_INPUT_DELIVERED,
      SmelterEventType.AUDIO_INPUT_DELIVERED,
      SmelterEventType.VIDEO_INPUT_PLAYING,
      SmelterEventType.AUDIO_INPUT_PLAYING,
      SmelterEventType.VIDEO_INPUT_EOS,
      SmelterEventType.AUDIO_INPUT_EOS,
    ].includes(event.type)
  ) {
    return { type: event.type, inputRef: parseInputRef(event.input_id) };
  } else if (SmelterEventType.OUTPUT_DONE === event.type) {
    return { type: event.type, outputId: event.output_id };
  } else {
    logger.error(`Unknown event type: ${event.type}`);
    return null;
  }
}
