import { OUTPUT_SPECIFIC_IMAGE_TYPE, type ImageRef } from './imageRef.js';
import { OUTPUT_SPECIFIC_INPUT_TYPE, type InputRef } from './inputRef.js';

type Ref = InputRef | ImageRef;

export function areRefsEqual(ref1: Ref, ref2: Ref): boolean {
  const sameType = ref1.type === ref2.type;
  const sameId = ref1.id === ref2.id;
  if (
    (ref1.type === OUTPUT_SPECIFIC_INPUT_TYPE && ref2.type === OUTPUT_SPECIFIC_INPUT_TYPE) ||
    (ref1.type === OUTPUT_SPECIFIC_IMAGE_TYPE && ref2.type === OUTPUT_SPECIFIC_IMAGE_TYPE)
  ) {
    return sameId && sameType && ref1.outputId === ref2.outputId;
  } else {
    return sameId && sameType;
  }
}
