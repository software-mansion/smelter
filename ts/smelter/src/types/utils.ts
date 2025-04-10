import { imageAssetTypes, type ImageAssetType } from './resource.js';

export function isValidImageType(type: any): type is ImageAssetType {
  return imageAssetTypes.includes(type);
}
