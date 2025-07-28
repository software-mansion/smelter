import Smelter from '@swmansion/smelter-node';
import App from './App';
import { sleep, spawn } from './utils';
import type { StoreApi } from 'zustand';
import type { RoomStore } from './store';

export const SmelterInstance = new Smelter();

export async function initializeSmelterInstance() {
  await SmelterInstance.init();

  if (process.env.ENVIRONMENT !== 'production') {
    void spawn(
      'bash',
      [
        '-c',
        'docker run -e UDP_MUX_PORT=8080  -e NETWORK_TEST_ON_START=false  -e NAT_1_TO_1_IP=127.0.0.1 -p 8080:8080 -p 8080:8080/udp  seaduboi/broadcast-box',
      ],
      {}
    );
  }

  while (true) {
    await sleep(500);
    try {
      const result = await fetch('http://127.0.0.1:8080/api/status');
      console.log(`connecting to broadcaster /api/status (response: ${await result.text()})`);
      if (result.ok) {
        break;
      }
    } catch (err) {
      console.log(`connecting to broadcast /api/status err (response: ${err})`);
    }
  }

  await SmelterInstance.registerOutput('output_1', <App />, {
    type: 'whip',
    endpointUrl: 'http://127.0.0.1:8080/api/whip',
    bearerToken: 'example',
    video: {
      encoderPreferences: [
        {
          type: 'ffmpeg_vp9',
        },
      ],
      resolution: {
        width: 1920,
        height: 1080,
      },
    },
    audio: true,
  });

  await SmelterInstance.start();
}

export class SmelterManager {
  private instance: Smelter;

  constructor() {
    this.instance = new Smelter();
  }

  public async startNewOutput(roomId: string, store: StoreApi<RoomStore>): Promise<SmelterOutput> {

  }
}

export class SmelterOutput {
  private instance: Smelter;
  public readonly url: string;

  constructor(instance: Smelter, url: string) {
    this.instance = instance;
    this.url = url;
  }

  public async registerInput();
}
