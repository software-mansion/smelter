import { OfflineSmelter } from '@swmansion/smelter-node';
import { View, Text, Rescaler, SlideShow, Slide, Mp4, InputStream } from '@swmansion/smelter';
import { downloadAllAssets } from './utils';
import path from 'path';
import type { ReactElement } from 'react';

function ExampleApp() {
  return (
    <SlideShow>
      <Slide durationMs={3000}>
        <TitleSlide text="First slide show" />
      </Slide>
      <Slide>
        <ExampleScene />
      </Slide>
      <Slide durationMs={3000}>
        <TitleSlide text="Second slide show" />
      </Slide>
      <Slide>
        <ExampleScene />
      </Slide>
    </SlideShow>
  );
}

function ExampleScene() {
  return (
    <View>
      <SlideShow>
        <Slide>
          <TitleSlide text="Part 1" />
        </Slide>
        <Slide durationMs={3000}>
          <SlideWithLabel label="BigBuckBunny sample video as <InputStream />">
            <InputStream inputId="input_1" />
          </SlideWithLabel>
        </Slide>
        <Slide>
          <TitleSlide text="Part 2" />
        </Slide>
        <Slide>
          <SlideWithLabel label="ForBiggerEscapes sample video as <Mp4 />">
            <Mp4
              source="https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerEscapes.mp4"
              volume={0.8}
            />
          </SlideWithLabel>
        </Slide>
        <Slide>
          <TitleSlide text="Part 3" />
        </Slide>
        <Slide>
          <SlideWithLabel label="ForBiggerBlazes sample video as <Mp4 />">
            <Mp4
              source="https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerBlazes.mp4"
              volume={0.8}
            />
          </SlideWithLabel>
        </Slide>
        <Slide durationMs={3000}>
          <TitleSlide text="The end" />
        </Slide>
      </SlideShow>
    </View>
  );
}

function TitleSlide(props: { text: string }) {
  return (
    <Rescaler>
      <Text
        style={{
          fontSize: 800,
          color: '#FF0000',
          lineHeight: 800,
          backgroundColor: '#FFFFFF88',
        }}>
        {props.text}
      </Text>
    </Rescaler>
  );
}

function SlideWithLabel({ label, children }: { label: string; children: ReactElement }) {
  return (
    <View>
      <Rescaler>{children}</Rescaler>
      <View style={{ bottom: 10, left: 10, height: 50 }}>
        <Text
          style={{ fontSize: 40, color: '#FF0000', lineHeight: 50, backgroundColor: '#FFFFFF88' }}>
          {label}
        </Text>
      </View>
    </View>
  );
}

async function run() {
  await downloadAllAssets();
  const smelter = new OfflineSmelter();
  await smelter.init();

  await smelter.registerInput('input_1', {
    type: 'mp4',
    serverPath: path.join(__dirname, '../.assets/BigBuckBunny.mp4'),
    offsetMs: 0,
    required: true,
  });

  await smelter.render(<ExampleApp />, {
    type: 'mp4',
    serverPath: path.join(__dirname, '../.assets/concat_mp4_output.mp4'),
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
    audio: {
      channels: 'stereo',
      encoder: {
        type: 'aac',
      },
    },
  });
}
void run();
