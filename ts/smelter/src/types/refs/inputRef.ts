/**
 * Represents ID of an input, it can mean either:
 * - Input registered with `registerInput` method.
 * - Input that was registered internally by components like <Mp4 />.
 */
export const OUTPUT_SPECIFIC_INPUT_TYPE = '__output_specific_input' as const;
const OUTPUT_SPECIFIC_INPUT_PREFIX = `${OUTPUT_SPECIFIC_INPUT_TYPE}:`;

export type InputRef =
  | {
      // Maps directly to "{id}" in HTTP API
      type: 'global';
      id: string;
    }
  | {
      // Maps to "__output_specific_input:{id}:{outputId}" in HTTP API
      type: typeof OUTPUT_SPECIFIC_INPUT_TYPE;
      outputId: string;
      id: number;
    };

export function inputRefIntoRawId(inputRef: InputRef): string {
  if (inputRef.type == 'global') {
    return inputRef.id;
  } else {
    return `${OUTPUT_SPECIFIC_INPUT_PREFIX}${inputRef.id}:${inputRef.outputId}`;
  }
}

export function assertGlobalInputId(id: string): void {
  if (id.startsWith(OUTPUT_SPECIFIC_INPUT_PREFIX)) {
    throw new Error(
      `Input id "${id}" is reserved: ids must not start with "${OUTPUT_SPECIFIC_INPUT_PREFIX}".`
    );
  }
}

export function parseInputRef(rawId: string): InputRef {
  if (rawId.startsWith(OUTPUT_SPECIFIC_INPUT_PREFIX)) {
    const rest = rawId.slice(OUTPUT_SPECIFIC_INPUT_PREFIX.length);
    const split = rest.split(':');
    if (split.length < 2) {
      throw new Error(`Invalid input ID. (${rawId})`);
    }
    return {
      type: OUTPUT_SPECIFIC_INPUT_TYPE,
      id: Number(split[0]),
      outputId: split.slice(1).join(':'),
    };
  }
  return { type: 'global', id: rawId };
}
