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
import ora from 'ora';

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
      <Text style={{ fontSize: 48, color: '#F24664' }}>
        {minutes.toFixed(0).padStart(2, '0')}:{seconds.toFixed(1).padStart(4, '0')}
      </Text>
    </View>
  );
}

function FirstVideo() {
  return (
    <Rescaler>
      <Mp4 source="https://smelter.dev/videos/template-scene-race.mp4" />
    </Rescaler>
  );
}

function SecondVideo() {
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

  const spinner = ora('Rendering. It can take up to a few minutes.');
  await smelter.render(<App />, {
    type: 'mp4',
    serverPath: './output.mp4',
    video: {
      encoder: {
        type: 'ffmpeg_h264',
        // 'ultrafast' is good for development. For production render select
        // slower (higher quality) preset e.g. 'medium'.
        preset: 'ultrafast',
      },
      resolution: {
        width: 1920,
        height: 1080,
      },
    },
    audio: {
      channels: 'stereo',
      encoder: { type: 'aac' },
    },
  });

  spinner.succeed(`Mp4 successfully written to ./output.mp4`);
}
void run();
