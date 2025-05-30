import type { Api } from '@swmansion/smelter';

export type Resolution = Api.Resolution;
export type ImageSpec = Required<Pick<Api.ImageSpec, 'url'>>;
export type ShaderSpec = Api.ShaderSpec;
export type Component = Extract<
  Api.Component,
  { type: 'input_stream' | 'view' | 'rescaler' | 'image' | 'text' | 'tiles' }
>;
export type RendererId = Api.RendererId;
export type InputId = Api.InputId;
export type OutputId = string;
