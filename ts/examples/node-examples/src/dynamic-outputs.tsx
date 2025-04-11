import Smelter from '@swmansion/smelter-node';
import { Text, InputStream, Tiles, Rescaler, View, useInputStreams } from '@swmansion/smelter';
import { downloadAllAssets, ffplayStartRtmpServerAsync, sleep } from './utils';
import path from 'path';
import { mkdirp } from 'fs-extra';

function ExampleApp() {
  const inputs = useInputStreams();
  return (
    <Tiles transition={{ durationMs: 200 }}>
      {Object.values(inputs).map(input => (
        <InputTile key={input.inputId} inputId={input.inputId} />
      ))}
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
  await mkdirp(path.join(__dirname, '../.workingdir'));
  await downloadAllAssets();
  const smelter = new Smelter();
  await smelter.init();

  const RESOLUTION = {
    width: 1920,
    height: 1080,
  } as const;
  const VIDEO_ENCODER_OPTS = {
    type: 'ffmpeg_h264',
    preset: 'ultrafast',
  } as const;

  await ffplayStartRtmpServerAsync(9002);

  await smelter.registerOutput('output_stream', <ExampleApp />, {
    type: 'rtmp_client',
    url: 'rtmp://127.0.0.1:9002',
    video: {
      encoder: VIDEO_ENCODER_OPTS,
      resolution: RESOLUTION,
    },
    audio: {
      encoder: {
        type: 'aac',
        channels: 'stereo',
      },
    },
  });

  await smelter.registerOutput('output_recording', <ExampleApp />, {
    type: 'mp4',
    serverPath: path.join(__dirname, '../.workingdir/dynamic_outputs_recording.mp4'),
    video: {
      encoder: VIDEO_ENCODER_OPTS,
      resolution: RESOLUTION,
    },
    audio: {
      encoder: {
        type: 'aac',
        channels: 'stereo',
      },
    },
  });

  await smelter.registerInput('input_1', {
    type: 'mp4',
    serverPath: path.join(__dirname, '../.assets/BigBuckBunny.mp4'),
  });

  console.log(
    'Start Smelter pipeline with single input ("input_1") and two outputs (RTP "output_stream" and MP4 "output_recording").'
  );
  await smelter.start();

  await sleep(10_000);
  console.log('Connect new input ("input_2") and start new output to MP4 "output_recording_part2"');
  await smelter.registerInput('input_2', {
    type: 'mp4',
    serverPath: path.join(__dirname, '../.assets/ElephantsDream.mp4'),
  });
  await smelter.registerOutput('output_recording_part2', <ExampleApp />, {
    type: 'mp4',
    serverPath: path.join(__dirname, '../.workingdir/dynamic_outputs_recording_10s.mp4'),
    video: {
      encoder: VIDEO_ENCODER_OPTS,
      resolution: RESOLUTION,
    },
    audio: {
      encoder: {
        type: 'aac',
        channels: 'stereo',
      },
    },
  });

  await sleep(10_000);
  console.log('Stop output "output_recording"');
  await smelter.unregisterOutput('output_recording');

  await sleep(10_000);
  console.log('Stop all remaining outputs.');
  await smelter.unregisterOutput('output_recording_part2');
  await smelter.unregisterOutput('output_stream');
  await smelter.terminate();
}
void run();
