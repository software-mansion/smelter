import { OfflineSmelter } from '@swmansion/smelter-node';
import {
  View,
  Text,
  SlideShow,
  Slide,
  Rescaler,
  Mp4,
  useAfterTimestamp,
  useCurrentTimestamp,
} from '@swmansion/smelter';
import { useEffect, useState } from 'react';
import chalk from 'chalk';

function useLogOnMount(msg: string) {
  const currentTimestamp = useCurrentTimestamp();
  useEffect(() => {
    const time = (currentTimestamp / 1000).toFixed(1);
    console.log(`- [${time}s] ${msg}`);
  }, []);
}

function Instructions() {
  return (
    <View
      style={{
        direction: 'column',
        top: 20,
        right: 20,
        padding: 20,
        width: 800,
        height: 200,
        backgroundColor: '#161127',
      }}>
      <Text style={{ fontSize: 50, lineHeight: 80 }}>Open index.tsx and get started.</Text>
      <Text style={{ fontSize: 30, lineHeight: 35, width: 800, wrap: 'word' }}>
        This is an example of an offline processing with Smelter. It shows few videos that run one
        after the other with a timer.
      </Text>
    </View>
  );
}

function TitleSlide(props: { title: string; text: string }) {
  useLogOnMount(`Show text slide (${props.title}: ${props.text})`);
  return (
    <View style={{ direction: 'column', paddingLeft: 200 }}>
      <View />
      <Text style={{ fontSize: 80, lineHeight: 100, color: '#F24664' }}>{props.title}</Text>
      <Text style={{ fontSize: 40 }}>{props.text}</Text>
      <View />
    </View>
  );
}

function Timer() {
  useLogOnMount(`Adding <Timer /> component`);

  const currentTimestamp = useCurrentTimestamp();
  const [startTimestamp, _setStartTimestamp] = useState(currentTimestamp);
  const [nextTimestamp, setNextTimestamp] = useState(0);
  const isAfter = useAfterTimestamp(startTimestamp + nextTimestamp);

  useEffect(() => {
    if (isAfter) {
      setNextTimestamp(nextTimestamp + 100);
    }
  }, [isAfter, nextTimestamp]);

  const minutes = Math.floor(nextTimestamp / 60_000);
  const seconds = nextTimestamp / 1000 - 60 * minutes;
  return (
    <View
      style={{
        bottom: 20,
        left: 20,
        height: 50,
        width: 200,
        paddingHorizontal: 28,
        paddingVertical: 20,
        backgroundColor: '#161127',
        borderRadius: 44,
      }}>
      <Text style={{ fontSize: 48, color: 'red' }}>
        {minutes.toFixed(0).padStart(2, '0')}:{seconds.toFixed(1).padStart(4, '0')}
      </Text>
    </View>
  );
}

function FirstVideo() {
  useLogOnMount(`Show video (racing game)`);
  return (
    <Rescaler>
      <Mp4 source="https://smelter.dev/videos/template-scene-race.mp4" />
    </Rescaler>
  );
}

function SecondVideo() {
  useLogOnMount(`Show video (streamer camera + gameplay)`);
  return (
    <>
      <Rescaler>
        <Mp4 source="https://smelter.dev/videos/template-scene-gameplay.mp4" />
      </Rescaler>
      <Rescaler
        style={{
          top: 20,
          left: 20,
          borderRadius: 44,
          rescaleMode: 'fill',
          height: 200,
          width: 355,
        }}>
        <Mp4 source="https://smelter.dev/videos/template-scene-streamer.mp4" />
      </Rescaler>
    </>
  );
}

function App() {
  useEffect(() => {
    return () => {
      console.log(`- React render complete`);
      console.log();
      console.log('Generating MP4 file ...');
    };
  }, []);
  return (
    <View style={{ backgroundColor: '#161127' }}>
      <SlideShow>
        <Slide durationMs={2000}>
          <TitleSlide title="Example 1" text="Racing game gameplay." />
        </Slide>
        <Slide>
          <FirstVideo />
        </Slide>
        <Slide durationMs={2000}>
          <TitleSlide title="Example 2" text="Streamer camera + gameplay." />
        </Slide>
        <Slide>
          <SecondVideo />
        </Slide>
      </SlideShow>
      <Instructions />
      <Timer />
    </View>
  );
}

async function run() {
  const smelter = new OfflineSmelter();
  await smelter.init();
  console.log('✔ Started offline smelter instance.');

  console.log();
  console.log('Starting rendering.');
  await smelter.render(<App />, {
    type: 'mp4',
    serverPath: './output.mp4',
    video: {
      encoder: { type: 'ffmpeg_h264', preset: 'ultrafast' },
      resolution: {
        width: 1920,
        height: 1080,
      },
    },
    audio: {
      encoder: { type: 'aac', channels: 'stereo' },
    },
  });

  console.log();
  console.log(chalk.green(`Mp4 successfully written to ${chalk.bold('./output.mp4')}`));
}
void run();
