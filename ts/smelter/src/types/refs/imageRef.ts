/**
 * Represents ID of an image, it can mean either:
 * - Image registered with `registerImage` method.
 * - Image that was registered automatically by an <Image /> component.
 */
export const OUTPUT_SPECIFIC_IMAGE_TYPE = '__output_specific_image' as const;
const OUTPUT_SPECIFIC_IMAGE_PREFIX = `${OUTPUT_SPECIFIC_IMAGE_TYPE}:`;

export type ImageRef =
  | {
      // Maps directly to "{id}" in HTTP API
      type: 'global';
      id: string;
    }
  | {
      // Maps to "__output_specific_image:{id}:{outputId}" in HTTP API
      type: typeof OUTPUT_SPECIFIC_IMAGE_TYPE;
      outputId: string;
      id: number;
    };

export function imageRefIntoRawId(imageRef: ImageRef): string {
  if (imageRef.type == 'global') {
    return imageRef.id;
  } else {
    return `${OUTPUT_SPECIFIC_IMAGE_PREFIX}${imageRef.id}:${imageRef.outputId}`;
  }
}

export function assertGlobalImageId(id: string): void {
  if (id.startsWith(OUTPUT_SPECIFIC_IMAGE_PREFIX)) {
    throw new Error(
      `Image id "${id}" is reserved: ids must not start with "${OUTPUT_SPECIFIC_IMAGE_PREFIX}".`
    );
  }
}

export function parseImageRef(rawId: string): ImageRef {
  if (rawId.startsWith(OUTPUT_SPECIFIC_IMAGE_PREFIX)) {
    const rest = rawId.slice(OUTPUT_SPECIFIC_IMAGE_PREFIX.length);
    const split = rest.split(':');
    if (split.length < 2) {
      throw new Error(`Invalid image ID. (${rawId})`);
    }
    return {
      type: OUTPUT_SPECIFIC_IMAGE_TYPE,
      id: Number(split[0]),
      outputId: split.slice(1).join(':'),
    };
  }
  return { type: 'global', id: rawId };
}
