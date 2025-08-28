import type { Output } from '@swmansion/smelter-core';
import type { Api, Renderers } from '@swmansion/smelter';

export type RegisterImage = {
  assetType: 'png' | 'jpeg' | 'svg' | 'gif' | 'auto';
  url: string;
};

export type RegisterShader = Renderers.RegisterShader;

export type RegisterOutput =
  | {
      type: 'stream';
      video: {
        resolution: Api.Resolution;
      };
      audio?: boolean;
    }
  | {
      type: 'canvas';
      video: {
        canvas: HTMLCanvasElement;
        resolution: Api.Resolution;
      };
      audio?: boolean;
    }
  | {
      type: 'whip_client';
      /**
       * WHIP server endpoint.
       */
      endpointUrl: string;
      /**
       * Token for authenticating communication with the WHIP server.
       */
      bearerToken?: string;
      iceServers?: RTCConfiguration['iceServers'];
      video: {
        resolution: Api.Resolution;
        maxBitrate?: number;
      };
      audio?: boolean;
    };

export function intoRegisterOutputRequest(request: RegisterOutput): Output.RegisterOutput {
  if (request.type === 'stream') {
    return { ...request, type: 'web-wasm-stream' };
  } else if (request.type === 'canvas') {
    return {
      ...request,
      type: 'web-wasm-canvas',
    };
  } else if (request.type === 'whip_client') {
    return { ...request, type: 'web-wasm-whip' };
  }
  throw new Error('Unknown output type');
}

export type RegisterInput =
  | { type: 'mp4'; url?: string; blob?: Blob }
  | { type: 'camera' }
  | { type: 'screen_capture' }
  | { type: 'stream'; stream: MediaStream }
  | { type: 'whep_client'; endpointUrl: string; bearerToken?: string };
