import path from 'path';
import type { StoreApi } from 'zustand';
import Smelter from '@swmansion/smelter-node';

import App from './app/App';
import type { RoomStore } from './app/store';
import { createRoomStore } from './app/store';
import { config } from './config';
import fs from 'fs-extra';
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
    }
  | {
      type: 'hls';
      url: string;
    };

// TODO: optional based on env
const DECODER_MAP = {
  h264: config.h264Decoder,
} as const;

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

    for (const shader of shadersController.shaders) {
      await this.registerShaderFromFile(
        SmelterInstance['instance'],
        shader.id,
        path.join(__dirname, `../shaders/${shader.shaderFile}`)
      );
    }

    setInterval(async () => {
      const smelterState = await SmelterInstance['instance'].status();
      console.log(JSON.stringify(smelterState, null, 2));
    }, 60_000 * 5);
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

  public async registerInput(inputId: string, opts: RegisterSmelterInputOptions): Promise<void> {
    try {
      if (opts.type === 'mp4') {
        await this.instance.registerInput(inputId, {
          type: 'mp4',
          serverPath: opts.filePath,
          decoderMap: DECODER_MAP,
          loop: true,
        });
      } else if (opts.type === 'hls') {
        await this.instance.registerInput(inputId, {
          type: 'hls',
          url: opts.url,
          decoderMap: DECODER_MAP,
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
          return;
        }
      }
      console.log(err.body, err);
      throw err;
    }
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

  private async registerShaderFromFile(smelter: Smelter, shaderId: string, file: string) {
    const source = await fs.readFile(file, { encoding: 'utf-8' });
  
    await smelter.registerShader(shaderId, {
      source,
    });
  }
}



export const SmelterInstance = new SmelterManager();
