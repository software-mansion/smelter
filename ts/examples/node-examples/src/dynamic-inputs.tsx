import Smelter from '@swmansion/smelter-node';
import { useInputStreams, Text, InputStream, Tiles, Rescaler, View } from '@swmansion/smelter';
import { downloadAllAssets, ffplayStartRtmpServerAsync, sleep } from './utils';
import path from 'path';

function ExampleApp() {
  const inputs = useInputStreams();
  return (
    <Tiles transition={{ durationMs: 200 }}>
      {Object.values(inputs).map(input =>
        !input.videoState ? (
          <Text key={input.inputId} style={{ fontSize: 40 }}>
            Waiting for stream {input.inputId} to connect
          </Text>
        ) : input.videoState === 'playing' ? (
          <InputTile key={input.inputId} inputId={input.inputId} />
        ) : input.videoState === 'finished' ? (
          <Text key={input.inputId} style={{ fontSize: 40 }}>
            Stream {input.inputId} finished
          </Text>
        ) : (
          'Fallback'
        )
      )}
    </Tiles>
  );
}

function InputTile({ inputId }: { inputId: string }) {
  return (
    <View>
      <Rescaler>
        <InputStream inputId={inputId} />
      </Rescaler>
      <View style={{ bottom: 10, left: 10, height: 50 }}>
        <Text
          style={{ fontSize: 40, color: '#FF0000', lineHeight: 50, backgroundColor: '#FFFFFF88' }}>
          Input ID: {inputId}
        </Text>
      </View>
    </View>
  );
}

async function run() {
  await downloadAllAssets();
  const smelter = new Smelter();
  await smelter.init();

  await ffplayStartRtmpServerAsync(9002);

  await smelter.registerOutput('output_1', <ExampleApp />, {
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
    audio: {
      channels: 'stereo',
      encoder: {
        type: 'aac',
      },
    },
  });
  await smelter.start();

  while (true) {
    await sleep(5000);
    await smelter.registerInput('input_1', {
      type: 'mp4',
      serverPath: path.join(__dirname, '../.assets/BigBuckBunny.mp4'),
    });

    await sleep(5000);
    await smelter.registerInput('input_2', {
      type: 'mp4',
      serverPath: path.join(__dirname, '../.assets/ElephantsDream.mp4'),
    });

    await sleep(5000);
    await smelter.unregisterInput('input_1');

    await sleep(5000);
    await smelter.unregisterInput('input_2');
  }
}
void run();
