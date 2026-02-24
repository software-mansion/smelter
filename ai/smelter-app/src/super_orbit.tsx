import Smelter from '@swmansion/smelter-node';
import { View, InputStream, Rescaler } from '@swmansion/smelter';
import { useState, useEffect } from 'react';
import { ffplayStartPlayerAsync } from './smelterFfplayHelper';

const WIDTH = 1920;
const HEIGHT = 1080;

const STREAM_WIDTH = 640;
const STREAM_HEIGHT = 360;
const SCALED_WIDTH = Math.round(STREAM_WIDTH * 0.65);  // 416
const SCALED_HEIGHT = Math.round(STREAM_HEIGHT * 0.65); // 234
// Diamond path radii: rx > ry → acute corners on left/right, obtuse on top/bottom
const ORBIT_RADIUS_X = 400;
const ORBIT_RADIUS_Y = 240;
// One full revolution every 6 seconds
const ANGULAR_VELOCITY = (2 * Math.PI) / 6000;
const TICK_MS = 16;

// Stream i becomes visible after APPEAR_DELAYS[i] milliseconds
const APPEAR_DELAYS = [10_000, 12_000, 14_000, 16_000];

// Diamond path: r(t) = 1 / (|cos(t)|/rx + |sin(t)|/ry)
function diamondPos(t: number, rx: number, ry: number) {
  const r = 1 / (Math.abs(Math.cos(t)) / rx + Math.abs(Math.sin(t)) / ry);
  return { x: r * Math.cos(t), y: r * Math.sin(t) };
}

function App() {
  const [angle, setAngle] = useState(0);
  const [visible, setVisible] = useState([false, false, false, false]);

  useEffect(() => {
    const timer = setInterval(() => {
      setAngle(a => (a + ANGULAR_VELOCITY * TICK_MS) % (2 * Math.PI));
    }, TICK_MS);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    const timers = APPEAR_DELAYS.map((delay, i) =>
      setTimeout(() => {
        setVisible(v => {
          const next = [...v];
          next[i] = true;
          return next;
        });
      }, delay)
    );
    return () => timers.forEach(clearTimeout);
  }, []);

  const cx = WIDTH / 2;
  const cy = HEIGHT / 2;
  const hw = SCALED_WIDTH / 2;
  const hh = SCALED_HEIGHT / 2;

  // 4 streams evenly spaced at 90° intervals along a diamond path
  const positions = [0, Math.PI / 2, Math.PI, (3 * Math.PI) / 2].map(offset => {
    const { x, y } = diamondPos(angle + offset, ORBIT_RADIUS_X, ORBIT_RADIUS_Y);
    return { left: cx + x - hw, top: cy + y - hh };
  });

  return (
    <View style={{ width: WIDTH, height: HEIGHT, overflow: 'visible' }}>
      {positions.map((pos, i) =>
        visible[i] ? (
          <Rescaler key={i} style={{ width: SCALED_WIDTH, height: SCALED_HEIGHT, top: pos.top, left: pos.left, rescaleMode: 'fit' }}>
            <InputStream inputId={`input_${i + 1}`} />
          </Rescaler>
        ) : null
      )}
    </View>
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
    if (data.toString() === '\x03') process.exit();
  });
}

void run();
