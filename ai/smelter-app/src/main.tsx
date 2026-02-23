import Smelter from '@swmansion/smelter-node';
import { useState, useEffect } from 'react';
import { EventEmitter } from 'events';
import { ffplayStartPlayerAsync } from './smelterFfplayHelper';
import MainLeftLayout from './main_left';
import Pip from './pip';

const switchEmitter = new EventEmitter();

const WIDTH = 1920;
const HEIGHT = 1080;

function App() {
  const [showPip, setShowPip] = useState(false);

  useEffect(() => {
    const handler = () => setShowPip(v => !v);
    switchEmitter.on('switch', handler);
    return () => { switchEmitter.off('switch', handler); };
  }, []);

  if (showPip) {
    return (
      <Pip
        mainInputId="input_1"
        topLeftInputId="input_2"
        topRightInputId="input_3"
        bottomLeftInputId="input_4"
        bottomRightInputId="input_5"
      />
    );
  }

  return (
    <MainLeftLayout
      mainInputId="input_1"
      firstInputId="input_2"
      secondInputId="input_3"
      thirdInputId="input_4"
      fourthInputId="input_5"
    />
  );
}

async function run() {
  const smelter = new Smelter();
  await smelter.init();

  await smelter.registerInput('input_1', {
    type: 'rtp_stream',
    port: 10000,
    video: { decoder: 'ffmpeg_h264' },
  });

  await smelter.registerInput('input_2', {
    type: 'rtp_stream',
    port: 10002,
    video: { decoder: 'ffmpeg_h264' },
  });

  await smelter.registerInput('input_3', {
    type: 'rtp_stream',
    port: 10004,
    video: { decoder: 'ffmpeg_h264' },
  });

  await smelter.registerInput('input_4', {
    type: 'rtp_stream',
    port: 10006,
    video: { decoder: 'ffmpeg_h264' },
  });

  await smelter.registerInput('input_5', {
    type: 'rtp_stream',
    port: 10008,
    video: { decoder: 'ffmpeg_h264' },
  });

  await ffplayStartPlayerAsync(8001);

  await smelter.registerOutput('output_1', <App />, {
    type: 'rtmp_client',
    url: 'rtmp://127.0.0.1:8001',
    video: {
      encoder: { type: 'ffmpeg_h264' },
      resolution: { width: WIDTH, height: HEIGHT },
    },
  });

  await smelter.start();

  process.stdin.setRawMode(true);
  process.stdin.resume();
  process.stdin.on('data', (data) => {
    const key = data.toString();
    if (key === '\r' || key === '\n') {
      switchEmitter.emit('switch');
    } else if (key === '\x03') {
      process.exit();
    }
  });
}

void run();
