import path from 'path';
import { useEffect, useState } from 'react';
import Smelter from '@swmansion/smelter-node';
import { InputStream, Tiles } from '@swmansion/smelter';
import { ffplayStartRtmpServerAsync, sleep } from './utils';

function ExampleApp() {
  const [count, setCount] = useState(0);

  useEffect(() => {
    if (count > 4) {
      return;
    }
    const timeout = setTimeout(() => {
      setCount(count + 1);
    }, 5000);
    return () => {
      clearTimeout(timeout);
    };
  });

  return (
    <Tiles>
      <InputStream inputId="input_1" />
      <InputStream inputId="input_2" />
    </Tiles>
  );
}

async function run() {
  const smelter = new Smelter();
  await smelter.init();

  await ffplayStartRtmpServerAsync(9002);

  const input1 = await smelter.registerInput('input_1', {
    type: 'mp4',
    serverPath: path.join(__dirname, '../.assets/BigBuckBunny.mp4'),
    offsetMs: 0,
    required: true,
  });

  const input2 = await smelter.registerInput('input_2', {
    type: 'mp4',
    serverPath: path.join(__dirname, '../.assets/ElephantsDream.mp4'),
    offsetMs: 0,
    required: true,
  });

  await smelter.registerOutput('output_1', <ExampleApp />, {
    type: 'rtmp_client',
    url: 'rtmp://127.0.0.1:9002',
    video: {
      encoder: {
        type: 'ffmpeg_h264',
        preset: 'ultrafast',
      },
      resolution: {
        width: 1920,
        height: 1080,
      },
    },
  });
  await smelter.start();

  await input1.pause();
  await sleep(2000);
  await input2.pause();
  await sleep(2000);
  await input1.resume();
  await sleep(2000);
  await input2.resume();
}
void run();
