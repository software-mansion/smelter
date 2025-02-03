import { ffmpegSendVideoFromMp4 } from '../utils/ffmpeg';
import { downloadAsync } from '../utils/utils';
import fs from 'fs-extra';
import path from 'path';
import type { Transition } from 'live-compositor';
import { View, Image, Text, Rescaler, Shader, InputStream, useInputStreams } from 'live-compositor';
import LiveCompositor from '@live-compositor/node';
import { useEffect, useState } from 'react';
import { gstStartPlayer } from '../utils/gst';

const OUTPUT_RESOLUTION = {
  width: 1920,
  height: 1080,
};

const INPUT_PORT = 9002;
const OUTPUT_PORT = 9004;

const TV_PATH = path.join(__dirname, '../assets/green_screen_example.mp4');
const TV_URL = 'https://assets.mixkit.co/videos/28293/28293-1080.mp4';

const BUNNY_PATH = path.join(__dirname, '../assets/bunny.mp4');
const BUNNY_URL =
  'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4';

const BACKGROUND_IMAGE_URL =
  'https://raw.githubusercontent.com/membraneframework-labs/video_compositor_snapshot_tests/main/demo_assets/news_room.jpeg';
const LOGO_URL =
  'https://raw.githubusercontent.com/membraneframework-labs/video_compositor_snapshot_tests/main/demo_assets/logo.png';

const FIRST_TRANSTION = {
  durationMs: 1000,
  easingFunction: 'bounce',
} as const;

const SECOND_TRANSTION = {
  durationMs: 1000,
  easingFunction: 'linear',
} as const;

function App() {
  const inputs = useInputStreams();
  const [started, setStarted] = useState(false);
  const [bunnyState, setBunnyState] = useState<'outside' | 'inside' | 'final' | null>(null);

  useEffect(() => {
    if (!started && inputs['tv_input']?.videoState === 'playing') {
      setStarted(true);
    }
  }, [started, inputs]);

  useEffect(() => {
    if (!started) {
      return;
    }
    const timeout = setTimeout(() => {
      if (!bunnyState) {
        setBunnyState('outside');
      } else if (bunnyState === 'outside') {
        setBunnyState('inside');
      } else if (bunnyState === 'inside') {
        setBunnyState('final');
      }
    }, 5000);
    return () => clearTimeout(timeout);
  }, [bunnyState, started]);

  if (!started) {
    return <View />;
  }

  return (
    <View>
      <NewReport />
      {bunnyState === 'outside' ? (
        <Bunny
          mute
          top={0}
          left={OUTPUT_RESOLUTION.width}
          width={OUTPUT_RESOLUTION.width}
          height={OUTPUT_RESOLUTION.height}
        />
      ) : bunnyState === 'inside' ? (
        <Bunny
          top={0}
          left={0}
          width={OUTPUT_RESOLUTION.width}
          height={OUTPUT_RESOLUTION.height}
          transition={FIRST_TRANSTION}
        />
      ) : bunnyState === 'final' ? (
        <Bunny
          top={20}
          right={20}
          rotation={360}
          width={OUTPUT_RESOLUTION.width / 4}
          height={OUTPUT_RESOLUTION.height / 4}
          transition={SECOND_TRANSTION}
        />
      ) : undefined}
      <Logo />
      <BreakingNewsText />
    </View>
  );
}

type BunnyProps = {
  mute?: boolean;
  top?: number;
  left?: number;
  right?: number;
  rotation?: number;
  width?: number;
  height?: number;
  transition?: Transition;
};

function Bunny(props: BunnyProps) {
  const { mute, ...other } = props;
  return (
    <Rescaler {...other}>
      <InputStream inputId="bunny" mute={mute} />
    </Rescaler>
  );
}

function NewReport() {
  return (
    <Shader shaderId="remove_green_screen" resolution={OUTPUT_RESOLUTION}>
      <Rescaler width={OUTPUT_RESOLUTION.width} height={OUTPUT_RESOLUTION.height}>
        <InputStream inputId="tv_input" />
      </Rescaler>
      <Rescaler width={OUTPUT_RESOLUTION.width} height={OUTPUT_RESOLUTION.height}>
        <Image imageId="background" />
      </Rescaler>
    </Shader>
  );
}

function BreakingNewsText() {
  return (
    <View height={180} bottom={0} left={0} direction="column">
      <Text
        width={600}
        height={55}
        fontSize={50}
        weight="bold"
        align="center"
        color="#FFFFFF"
        backgroundColor="#FF0000">
        BREAKING NEWS
      </Text>
      <Text
        height={80}
        width={OUTPUT_RESOLUTION.width}
        fontSize={65}
        align="center"
        color="#FFFFFF"
        backgroundColor="#808080">
        LiveCompositor is rumored to allegedly compose video
      </Text>
      <View height={50}>
        <Text
          fontSize={40}
          width={200}
          height={50}
          align="center"
          color="#FFFFFF"
          backgroundColor="#000000">
          88:29
        </Text>
        <Text
          fontSize={40}
          width={OUTPUT_RESOLUTION.width - 200}
          height={50}
          align="center"
          color="#000000"
          backgroundColor="#FFFF00">
          Leaked docs can be found at https://smelter.dev/docs
        </Text>
      </View>
    </View>
  );
}

function Logo() {
  return (
    <View top={50} left={50}>
      <Image imageId="logo" />
    </View>
  );
}

async function exampleAsync() {
  await downloadAsync(TV_URL, TV_PATH);
  await downloadAsync(BUNNY_URL, BUNNY_PATH);

  const compositor = new LiveCompositor();
  await compositor.init();
  process.env.SMELTER_LOGGER_LEVEL = 'debug';

  await compositor.registerInput('tv_input', {
    type: 'rtp_stream',
    port: INPUT_PORT,
    video: {
      decoder: 'ffmpeg_h264',
    },
  });
  void ffmpegSendVideoFromMp4(INPUT_PORT, TV_PATH);

  await compositor.registerInput('bunny', {
    type: 'mp4',
    serverPath: BUNNY_PATH,
  });

  await compositor.registerShader('remove_green_screen', {
    source: await fs.readFile(path.join(__dirname, 'remove_green_screen.wgsl'), 'utf-8'),
  });

  await compositor.registerImage('background', {
    assetType: 'jpeg',
    url: BACKGROUND_IMAGE_URL,
  });

  await compositor.registerImage('logo', {
    assetType: 'png',
    url: LOGO_URL,
  });

  await compositor.registerOutput('output', {
    type: 'rtp_stream',
    port: OUTPUT_PORT,
    transportProtocol: 'tcp_server',
    video: {
      resolution: OUTPUT_RESOLUTION,
      encoder: {
        type: 'ffmpeg_h264',
        preset: 'ultrafast',
      },
      root: <App />,
    },
    audio: {
      encoder: {
        type: 'opus',
        channels: 'stereo',
      },
    },
  });
  gstStartPlayer(OUTPUT_PORT);

  await compositor.start();
}

void exampleAsync();
