import Smelter from '@swmansion/smelter-node';
import { View, Tiles, InputStream, useAudioInput } from '@swmansion/smelter';
import { useState, useEffect } from 'react';
import { EventEmitter } from 'events';
import { ffplayStartPlayerAsync } from './smelterFfplayHelper';

const swapEmitter = new EventEmitter();

const WIDTH = 1920;
const HEIGHT = 1080;

function App() {
  const [swapped, setSwapped] = useState(false);

  // When not swapped: input_1 left, input_2 right → play input_4 (audio for stream 2)
  // When swapped:     input_2 left, input_1 right → play input_3 (audio for stream 1)
  useAudioInput('input_3', { volume: swapped ? 1 : 0 });
  useAudioInput('input_4', { volume: swapped ? 0 : 1 });

  useEffect(() => {
    const handler = () => setSwapped(s => !s);
    swapEmitter.on('swap', handler);
    return () => { swapEmitter.off('swap', handler); };
  }, []);

  const stream1 = <View id="stream1" key="stream1"><InputStream inputId="input_1" /></View>;
  const stream2 = <View id="stream2" key="stream2"><InputStream inputId="input_2" /></View>;

  return (
    <Tiles
      id="main"
      style={{ width: WIDTH, height: HEIGHT }}
      transition={{ durationMs: 1000 }}
    >
      {swapped ? [stream2, stream1] : [stream1, stream2]}
    </Tiles>
  );
}

async function run() {
  const smelter = new Smelter();
  await smelter.init();

  // Stream 1 — video on port 10000
  await smelter.registerInput('input_1', {
    type: 'rtp_stream',
    port: 10000,
    video: { decoder: 'vulkan_h264' },
  });

  // Stream 2 — video on port 10002
  await smelter.registerInput('input_2', {
    type: 'rtp_stream',
    port: 10002,
    video: { decoder: 'vulkan_h264' },
  });

  // Stream 1 — audio on port 10004
  await smelter.registerInput('input_3', {
    type: 'rtp_stream',
    port: 10004,
    audio: { decoder: 'opus' },
  });

  // Stream 2 — audio on port 10006
  await smelter.registerInput('input_4', {
    type: 'rtp_stream',
    port: 10006,
    audio: { decoder: 'opus' },
  });

  await ffplayStartPlayerAsync(8001);

  await smelter.registerOutput('output_1', <App />, {
    type: 'rtmp_client',
    url: 'rtmp://127.0.0.1:8001',
    video: {
      encoder: { type: 'vulkan_h264' },
      resolution: { width: WIDTH, height: HEIGHT },
    },
    audio: {
      encoder: { type: 'aac' },
    },
  });

  await smelter.start();

  process.stdin.setRawMode(true);
  process.stdin.resume();
  process.stdin.on('data', (data) => {
    const key = data.toString();
    if (key === '\r' || key === '\n') {
      swapEmitter.emit('swap');
    } else if (key === '\x03') {
      process.exit();
    }
  });
}

void run();
