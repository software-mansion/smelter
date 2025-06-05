import type { Api } from '../api';
import type { Renderers } from '@swmansion/smelter';

export function intoRegisterImage(image: Renderers.RegisterImage): Api.ImageSpec {
  const source = {
    url: image.url,
    path: image.serverPath,
  };
  // if (image.assetType === 'svg') {
  return {
    asset_type: image.assetType,
    resolution: image.resolution,
    ...source,
  };
  // } else {
  //   return {
  //     asset_type: image.assetType,
  //     ...source,
  //   };
  //d }
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
