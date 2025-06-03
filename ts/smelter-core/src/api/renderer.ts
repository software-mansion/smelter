import type { Api } from '../api';
import type { Renderers } from '@swmansion/smelter';

export function intoRegisterImage(image: Renderers.RegisterImage): Api.ImageSpec {
  const source = {
    url: image.url,
    path: image.serverPath,
  };
  if (image.assetType === 'svg') {
    return {
      asset_type: 'svg',
      resolution: image.resolution,
      ...source,
    };
  } else if (image.assetType === 'auto') {
    return {
      asset_type: 'auto',
      resolution: image.resolution,
      ...source,
    };
  } else {
    return {
      asset_type: image.assetType,
      ...source,
    };
  }
}

export function intoRegisterWebRenderer(
  renderer: Renderers.RegisterWebRenderer
): Api.WebRendererSpec {
  return {
    url: renderer.url,
    resolution: renderer.resolution,
    embedding_method: renderer.embeddingMethod,
  };
}
