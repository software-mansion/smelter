/**
 * Represents ID of an input, it can mean either:
 * - Input registered with `registerInput` method.
 * - Input that was registered internally by components like <Mp4 />.
 */
export type InputRef =
  | {
      // Maps to "global:{id}" in HTTP API
      type: 'global';
      id: string;
    }
  | {
      // Maps to "output-specific-input:{id}:{outputId}" in HTTP API
      type: 'output-specific-input';
      outputId: string;
      id: number;
    };

export function inputRefIntoRawId(inputRef: InputRef): string {
  if (inputRef.type == 'global') {
    return `global:${inputRef.id}`;
  } else {
    return `output-specific-input:${inputRef.id}:${inputRef.outputId}`;
  }
}

export function parseInputRef(rawId: string): InputRef {
  const split = rawId.split(':');
  if (split.length < 2) {
    throw new Error(`Invalid input ID. (${rawId})`);
  } else if (split[0] === 'global') {
    return {
      type: 'global',
      id: split.slice(1).join(':'),
    };
  } else if (split[0] === 'output-specific-input') {
    return {
      type: 'output-specific-input',
      id: Number(split[1]),
      outputId: split.slice(2).join(':'),
    };
  } else {
    throw new Error(`Unknown input type (${split[0]}).`);
  }
}
