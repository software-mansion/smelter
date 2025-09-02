/**
 * Represents ID of an image, it can mean either:
 * - Image registered with `registerImage` method.
 * - Image that was registered automatically by an <Image /> component.
 */
export type ImageRef =
  | {
      // Maps to "global:{id}" in HTTP API
      type: 'global';
      id: string;
    }
  | {
      // Maps to "output-specific-image:{id}:{outputId}" in HTTP API
      type: 'output-specific-image';
      outputId: string;
      id: number;
    };

export function imageRefIntoRawId(imageRef: ImageRef): string {
  if (imageRef.type == 'global') {
    return `global:${imageRef.id}`;
  } else {
    return `output-specific-image:${imageRef.id}:${imageRef.outputId}`;
  }
}

export function parseImageRef(rawId: string): ImageRef {
  const split = rawId.split(':');
  if (split.length < 2) {
    throw new Error(`Invalid image ID. (${rawId})`);
  } else if (split[0] === 'global') {
    return {
      type: 'global',
      id: split.slice(1).join(':'),
    };
  } else if (split[0] === 'output-specific-image') {
    return {
      type: 'output-specific-image',
      id: Number(split[1]),
      outputId: split.slice(2).join(':'),
    };
  } else {
    throw new Error(`Unknown image type (${split[0]}).`);
  }
}
