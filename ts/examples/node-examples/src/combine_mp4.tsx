import { OfflineSmelter } from '@swmansion/smelter-node';
import {
  View,
  Text,
  Tiles,
  Rescaler,
  InputStream,
  useCurrentTimestamp,
  useAfterTimestamp,
  Show,
} from '@swmansion/smelter';
import { downloadAllAssets } from './utils';
import path from 'path';
import { useState } from 'react';

function ExampleApp() {
  return (
    <Tiles transition={{ durationMs: 200 }}>
      <InputTile inputId="input_1" />
      <Show delayMs={2000}>
        <InputTile inputId="input_2" />
      </Show>
      <Show timeRangeMs={{ start: 5000, end: 8000 }}>
        <InputTile inputId="input_2" />
      </Show>
    </Tiles>
  );
}

function InputTile({ inputId }: { inputId: string }) {
  const currentTimestamp = useCurrentTimestamp();
  const [mountTime, _setMountTime] = useState(() => currentTimestamp);
  const afterDelay = useAfterTimestamp(mountTime + 1000);

  const bottom = afterDelay ? 10 : 1080;
  return (
    <View>
      <Rescaler>
        <InputStream inputId={inputId} />
      </Rescaler>
      <View style={{ bottom, left: 10, height: 50 }} transition={{ durationMs: 1000 }}>
        <Text
          style={{
            fontSize: 40,
            fontFamily: 'Noto Sans',
            color: '#FF0000',
            lineHeight: 50,
            backgroundColor: '#FFFFFF88',
          }}>
          Input ID: {inputId}
        </Text>
      </View>
    </View>
  );
}

async function run() {
  await downloadAllAssets();
  const smelter = new OfflineSmelter();
  await smelter.init();

  await smelter.registerFont(
    'https://fonts.gstatic.com/s/notosans/v36/o-0mIpQlx3QUlC5A4PNB6Ryti20_6n1iPHjcz6L1SoM-jCpoiyD9A-9a6Vc.ttf'
  );
  await smelter.registerInput('input_1', {
    type: 'mp4',
    serverPath: path.join(__dirname, '../.assets/BigBuckBunny.mp4'),
    offsetMs: 0,
    required: true,
  });

  await smelter.registerInput('input_2', {
    type: 'mp4',
    serverPath: path.join(__dirname, '../.assets/ElephantsDream.mp4'),
    offsetMs: 0,
    required: true,
  });

  await smelter.render(
    <ExampleApp />,
    {
      type: 'mp4',
      serverPath: path.join(__dirname, '../.assets/combing_mp4_output.mp4'),
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
    },
    10000
  );
}
void run();
