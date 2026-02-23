import Smelter from '@swmansion/smelter-node';
import { View, InputStream, useAudioInput } from '@swmansion/smelter';
import { useState, useEffect, useRef } from 'react';
import { EventEmitter } from 'events';
import { ffplayStartPlayerAsync } from './smelterFfplayHelper';

const swapEmitter = new EventEmitter();

const WIDTH = 1920;
const HEIGHT = 1080;

const FADE_PHASE_MS = 1000;
const TICK_MS = 16;

const STREAM_WIDTH = 640;
const STREAM_HEIGHT = 360;
const ORBIT_RADIUS = 320;
// One full revolution every 6 seconds
const ANGULAR_VELOCITY = (2 * Math.PI) / 6000;

function App() {
  const [angle, setAngle] = useState(0);
  const [vol1, setVol1] = useState(1); // input_3 starts audible
  const [vol2, setVol2] = useState(0); // input_4 starts muted

  const directionRef = useRef(1); // 1 = clockwise, -1 = counter-clockwise
  const activeAudioRef = useRef<1 | 2>(1);
  const audioTimerRef = useRef<NodeJS.Timeout | null>(null);

  useAudioInput('input_3', { volume: vol1 });
  useAudioInput('input_4', { volume: vol2 });

  // Continuously advance the orbit angle
  useEffect(() => {
    const timer = setInterval(() => {
      setAngle(a => a + directionRef.current * ANGULAR_VELOCITY * TICK_MS);
    }, TICK_MS);
    return () => clearInterval(timer);
  }, []);

  // ENTER: reverse orbit direction + cross-fade audio
  useEffect(() => {
    const handler = () => {
      directionRef.current = -directionRef.current;

      const wasActive = activeAudioRef.current;
      const nowActive = wasActive === 1 ? 2 : 1;
      activeAudioRef.current = nowActive;

      if (audioTimerRef.current) clearInterval(audioTimerRef.current);

      let t = 0;
      audioTimerRef.current = setInterval(() => {
        t += TICK_MS;
        if (t <= FADE_PHASE_MS) {
          const vol = 1 - t / FADE_PHASE_MS;
          wasActive === 1 ? setVol1(vol) : setVol2(vol);
        } else if (t <= 2 * FADE_PHASE_MS) {
          const vol = (t - FADE_PHASE_MS) / FADE_PHASE_MS;
          nowActive === 1 ? setVol1(vol) : setVol2(vol);
        } else {
          clearInterval(audioTimerRef.current!);
          audioTimerRef.current = null;
          nowActive === 1 ? (setVol1(1), setVol2(0)) : (setVol1(0), setVol2(1));
        }
      }, TICK_MS);
    };

    swapEmitter.on('swap', handler);
    return () => {
      swapEmitter.off('swap', handler);
      if (audioTimerRef.current) clearInterval(audioTimerRef.current);
    };
  }, []);

  const cx = WIDTH / 2;
  const cy = HEIGHT / 2;

  // Stream 1 at angle, stream 2 exactly opposite (angle + π)
  const x1 = cx + ORBIT_RADIUS * Math.cos(angle) - STREAM_WIDTH / 2;
  const y1 = cy + ORBIT_RADIUS * Math.sin(angle) - STREAM_HEIGHT / 2;
  const x2 = cx - ORBIT_RADIUS * Math.cos(angle) - STREAM_WIDTH / 2;
  const y2 = cy - ORBIT_RADIUS * Math.sin(angle) - STREAM_HEIGHT / 2;

  return (
    <View style={{ width: WIDTH, height: HEIGHT, overflow: 'visible' }}>
      <View style={{ width: STREAM_WIDTH, height: STREAM_HEIGHT, top: y1, left: x1 }}>
        <InputStream inputId="input_1" />
      </View>
      <View style={{ width: STREAM_WIDTH, height: STREAM_HEIGHT, top: y2, left: x2 }}>
        <InputStream inputId="input_2" />
      </View>
    </View>
  );
}

async function run() {
  const smelter = new Smelter();
  await smelter.init();

  // Stream 1 — video on port 10000, audio on port 10004
  await smelter.registerInput('input_1', {
    type: 'rtp_stream',
    port: 10000,
    video: { decoder: 'vulkan_h264' },
  });

  // Stream 2 — video on port 10002, audio on port 10006
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

  // Display output with `ffplay`.
  await ffplayStartPlayerAsync(8001);

  await smelter.registerOutput('output_1', <App />, {
    type: 'rtmp_client',
    url: 'rtmp://127.0.0.1:8001',
    video: {
      encoder: {
        type: 'vulkan_h264',
      },
      resolution: {
        width: WIDTH,
        height: HEIGHT,
      },
    },
    audio: {
      encoder: {
        type: 'aac',
      },
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
