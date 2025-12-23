import path from 'path';
import type { StoreApi } from 'zustand';
import Smelter from '@swmansion/smelter-node';

import App from './app/App';
import type { RoomStore } from './app/store';
import { createRoomStore } from './app/store';
import { config } from './config';
import { readFile } from 'fs-extra';
import shadersController from './shaders/shaders';

export type SmelterOutput = {
  id: string;
  url: string;
  store: StoreApi<RoomStore>;
};

export type RegisterSmelterInputOptions =
  | {
      type: 'mp4';
      filePath: string;
      loop?: boolean;
    }
  | {
      type: 'hls';
      url: string;
    }
  | {
      type: 'whip';
      url: string;
    };

// TODO: optional based on env
const MP4_DECODER_MAP = {
  h264: config.h264Decoder,
};

const WHIP_SERVER_DECODER_PREFERENCES = [config.h264Decoder];

export class SmelterManager {
  private instance: Smelter;

  constructor() {
    this.instance = new Smelter();
  }

  public async init() {
    await SmelterInstance['instance'].init();
    await SmelterInstance['instance'].start();
    await SmelterInstance['instance'].registerImage('spinner', {
      serverPath: path.join(__dirname, '../loading.gif'),
      assetType: 'gif',
    });
    await SmelterInstance['instance'].registerImage('news_strip', {
      serverPath: path.join(process.cwd(), 'mp4s', 'news_strip', 'news_strip.png'),
      assetType: 'png',
    });
    await SmelterInstance['instance'].registerImage('smelter_logo', {
      serverPath: path.join(__dirname, '../imgs/smelter_logo.png'),
      assetType: 'png',
    });

    for (const shader of shadersController.shaders) {
      await this.registerShaderFromFile(
        SmelterInstance['instance'],
        shader.id,
        path.join(__dirname, `../shaders/${shader.shaderFile}`)
      );
    }
    await SmelterInstance['instance'].registerFont(
      'https://fonts.googleapis.com/css2?family=Poppins:ital,wght@0,100;0,200;0,300;0,400;0,500;0,600;0,700;0,800;0,900;1,100;1,200;1,300;1,400;1,500;1,600;1,700;1,800;1,900&family=Roboto+Mono:ital,wght@0,100..700;1,100..700&display=swap'
    );
  }

  public async registerOutput(roomId: string): Promise<SmelterOutput> {
    let store = createRoomStore();
    await this.instance.registerOutput(roomId, <App store={store} />, {
      type: 'whep_server',
      video: {
        encoder: config.h264Encoder,
        resolution: {
          width: 2560,
          height: 1440,
        },
      },
      audio: {
        encoder: {
          type: 'opus',
        },
      },
    });

    return { id: roomId, url: `${config.whepBaseUrl}/${encodeURIComponent(roomId)}`, store };
  }

  public async unregisterOutput(roomId: string): Promise<void> {
    try {
      await this.instance.unregisterOutput(roomId);
    } catch (err: any) {
      if (err.body?.error_code === 'OUTPUT_STREAM_NOT_FOUND') {
        console.log(roomId, 'Output already removed');
        return;
      }
      console.log(err.body, err);
      throw err;
    }
  }

  public async registerInput(inputId: string, opts: RegisterSmelterInputOptions): Promise<string> {
    try {
      if (opts.type === 'whip') {
        const res = await this.instance.registerInput(inputId, {
          type: 'whip_server',
          video: { decoderPreferences: WHIP_SERVER_DECODER_PREFERENCES },
        });
        console.log('whipInput', res);
        return res.bearerToken;
      } else if (opts.type === 'mp4') {
        await this.instance.registerInput(inputId, {
          type: 'mp4',
          serverPath: opts.filePath,
          decoderMap: MP4_DECODER_MAP,
          loop: opts.loop ?? true,
        });
      } else if (opts.type === 'hls') {
        await this.instance.registerInput(inputId, {
          type: 'hls',
          url: opts.url,
          decoderMap: MP4_DECODER_MAP,
        });
      }
    } catch (err: any) {
      if (err.body?.error_code === 'INPUT_STREAM_ALREADY_REGISTERED') {
        throw new Error('already registered');
      }
      try {
        // try to unregister in case it worked
        await this.instance.unregisterInput(inputId);
      } catch (err: any) {
        if (err.body?.error_code === 'INPUT_STREAM_NOT_FOUND') {
          return '';
        }
      }
      console.log(err.body, err);
      throw err;
    }
    return '';
  }

  public async unregisterInput(inputId: string): Promise<void> {
    try {
      await this.instance.unregisterInput(inputId);
    } catch (err: any) {
      if (err.body?.error_code === 'INPUT_STREAM_NOT_FOUND') {
        console.log(inputId, 'Input already removed');
        return;
      }
      console.log(err.body, err);
      throw err;
    }
  }

  public async registerImage(
    imageId: string,
    opts: { serverPath?: string; url?: string; assetType: 'jpeg' | 'png' | 'gif' | 'svg' | 'auto' }
  ): Promise<void> {
    await this.instance.registerImage(imageId, {
      serverPath: opts.serverPath,
      url: opts.url,
      assetType: opts.assetType,
    });
  }

  private async registerShaderFromFile(smelter: Smelter, shaderId: string, file: string) {
    const source = await readFile(file, { encoding: 'utf-8' });

    await smelter.registerShader(shaderId, {
      source,
    });
  }
}

export const SmelterInstance = new SmelterManager();
