import Smelter from '@swmansion/smelter-node';
import App from './App';
import { sleep, spawn } from './utils';

export const SmelterInstance = new Smelter();

export async function initializeSmelterInstance() {
  await SmelterInstance.init();

  void spawn(
    'bash',
    [
      '-c',
      'docker run -e UDP_MUX_PORT=8080  -e NETWORK_TEST_ON_START=false  -e NAT_1_TO_1_IP=127.0.0.1 -p 8080:8080 -p 8080:8080/udp  seaduboi/broadcast-box',
    ],
    {}
  );
  await sleep(5000);

  await SmelterInstance.registerOutput('output_1', <App />, {
    type: 'whip',
    endpointUrl: 'http://127.0.0.1:8080/api/whip',
    bearerToken: 'example',
    video: {
      resolution: {
        width: 1920,
        height: 1080,
      },
    },
    audio: true,
  });

  await SmelterInstance.start();
}
