import type * as Api from '../api.js';

export type RegisterShader = Api.ShaderSpec;

export type RegisterImage = {
  url?: string;
  serverPath?: string;
  resolution?: Api.Resolution;
};

export type RegisterWebRenderer = {
  url: string;
  resolution: Api.Resolution;
  embeddingMethod?: Api.WebEmbeddingMethod;
};
