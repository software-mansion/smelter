import Smelter from '@swmansion/smelter-node';
import App from './App';
import { sleep, spawn } from './utils';

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
      console.log(`connecting (response: ${await result.text()})`);
      if (result.status < 300) {
        break;
      }
    } catch (err) {
      console.log(`connecting err (response: ${err})`);
    }
  }

  await SmelterInstance.registerOutput('output_1', <App />, {
    type: 'whip',
    endpointUrl: 'http://127.0.0.1:8080/api/whip',
    bearerToken: 'example',
    video: {
      encoderPreferences: [
        {
          type: 'ffmpeg_h264',
          preset: 'veryfast',
          ffmpegOptions: {
            tune: 'zerolatency',
          },
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
