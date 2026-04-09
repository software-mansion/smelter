import { useEffect, useState } from 'react';
import Smelter from '@swmansion/smelter-node';
import { View, Text, Mp4, Rescaler } from '@swmansion/smelter';
import WorkerdSmelterInstanceManager from './manager/workerdSmelterInstance';

function Scene() {
  const [seconds, setSeconds] = useState(0);

  useEffect(() => {
    const timeout = setTimeout(() => {
      setSeconds(seconds + 1);
    }, 1000);
    return () => clearTimeout(timeout);
  });

  return (
    <View style={{ direction: 'column' }}>
      <Rescaler>
        <Mp4 source="https://smelter.dev/videos/template-scene-race.mp4" />
      </Rescaler>
      <Text style={{ fontSize: 30, align: 'center', width: 1920 }}>Streaming for {seconds}s</Text>
    </View>
  );
}

async function run() {
  const smelter = new Smelter(
    new WorkerdSmelterInstanceManager({
      url: 'http://127.0.0.1:8081',
    })
  );
  await smelter.init();

  await smelter.registerOutput('output_1', <Scene />, {
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
}

let runPromise: Promise<void> | undefined;
addEventListener('fetch', (event: any) => {
  if (!runPromise) {
    runPromise = run().catch(console.error);
  }

  event.respondWith(new Response('started'));
  event.waitUntil(runPromise);
});
