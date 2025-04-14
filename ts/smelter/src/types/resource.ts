import type * as Api from '../api.js';

export type RegisterShader = Api.ShaderSpec;

export type RegisterImage =
  | {
      assetType: 'png';
      url?: string;
      serverPath?: string;
    }
  | {
      assetType: 'jpeg';
      url?: string;
      serverPath?: string;
    }
  | {
      assetType: 'svg';
      url?: string;
      serverPath?: string;
      resolution?: Api.Resolution;
    }
  | {
      assetType: 'gif';
      url?: string;
      serverPath?: string;
    };

export type ImageAssetType = RegisterImage['assetType'];

export const imageAssetTypes: ImageAssetType[] = ['png', 'jpeg', 'svg', 'gif'];

export type RegisterWebRenderer = {
  url: string;
  resolution: Api.Resolution;
  embeddingMethod?: Api.WebEmbeddingMethod;
};
