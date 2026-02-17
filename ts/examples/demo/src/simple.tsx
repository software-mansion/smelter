import Smelter, { ExistingInstanceManager } from '@swmansion/smelter-node';
import { Tiles, InputStream, useInputStreams } from '@swmansion/smelter';

function ExampleApp() {
  const inputs = useInputStreams();

  return (
    <Tiles>
      {Object.entries(inputs).map(([inputId, _]) => (
        <InputStream key={inputId} inputId={inputId} />
      ))}
    </Tiles>
  );
}

async function run() {
  const manager = new ExistingInstanceManager({
    url: 'https://puffer.fishjam.io/smelter-test/api',
    authorizationHeader: process.env.DEMO_AUTH_HEADER,
  });
  const smelter = new Smelter(manager);
  await smelter.init();
  await smelter.start();
  console.log('start');

  await smelter.registerOutput('output_1', <ExampleApp />, {
    type: 'whep_server',
    bearerToken: 'example',
    video: {
      encoder: {
        type: 'ffmpeg_h264',
        preset: 'ultrafast',
        ffmpegOptions: {
          tune: 'zerolatency',
          thread_type: 'slice',
        },
      },
      resolution: {
        width: 1920,
        height: 1080,
      },
    },
    audio: {
      encoder: {
        type: 'opus',
      },
    },
  });
  console.log(
    'https://puffer.fishjam.io/smelter-test/whep.html?url=https://puffer.fishjam.io/smelter-test/webrtc/whep/output_1&token=example'
  );

  await smelter.registerInput('input_ffmpeg_h264', {
    type: 'whip_server',
    bearerToken: 'example',
    video: {
      decoderPreferences: ['ffmpeg_h264'],
    },
  });
  console.log(
    'https://puffer.fishjam.io/smelter-test/whip.html?url=https://puffer.fishjam.io/smelter-test/webrtc/whip/input_ffmpeg_h264&token=example'
  );

  await smelter.registerInput('input_vulkan_h264', {
    type: 'whip_server',
    bearerToken: 'example',
    video: {
      decoderPreferences: ['vulkan_h264'],
    },
  });
  console.log(
    'https://puffer.fishjam.io/smelter-test/whip.html?url=https://puffer.fishjam.io/smelter-test/webrtc/whip/input_vulkan_h264&token=example'
  );

  await smelter.registerInput('mp4', {
    type: 'mp4',
    serverPath: '/bunny.mp4',
    decoderMap: {
      h264: 'vulkan_h264',
    },
    loop: true,
  });
}
void run();
