import type * as Api from '../api.js';

export type RegisterShader = Api.ShaderSpec;

export type RegisterImage = {
  assetType: 'png' | 'jpeg' | 'svg' | 'gif' | 'auto';
  url?: string;
  serverPath?: string;
};

export type ImageAssetType = RegisterImage['assetType'];

export const imageAssetTypes: ImageAssetType[] = ['png', 'jpeg', 'svg', 'gif', 'auto'];
export type RegisterWebRenderer = {
  url: string;
  resolution: Api.Resolution;
  embeddingMethod?: Api.WebEmbeddingMethod;
};
