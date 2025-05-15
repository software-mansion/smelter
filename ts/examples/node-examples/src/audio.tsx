import Smelter from '@swmansion/smelter-node';
import { Text, InputStream, Tiles, Rescaler, View } from '@swmansion/smelter';
import { downloadAllAssets, ffplayStartRtmpServerAsync } from './utils';
import path from 'path';
import { useState, useEffect } from 'react';

function ExampleApp() {
  const [streamWithAudio, setStream] = useState('input_1');
  useEffect(() => {
    const timeout = setTimeout(() => {
      setStream(streamWithAudio === 'input_1' ? 'input_2' : 'input_1');
    }, 5000);
    return () => clearTimeout(timeout);
  }, [streamWithAudio]);

  return (
    <Tiles transition={{ durationMs: 200 }}>
      <InputTile inputId="input_1" muted={streamWithAudio === 'input_1'} />
      <InputTile inputId="input_2" muted={streamWithAudio === 'input_2'} />
    </Tiles>
  );
}

function InputTile({ inputId, muted }: { inputId: string; muted: boolean }) {
  const [volume, setVolume] = useState(1.0);

  useEffect(() => {
    const timeout = setTimeout(() => {
      if (volume < 0.2) {
        setVolume(1.0);
      } else {
        setVolume(volume - 0.1);
      }
    }, 1000);
    return () => clearTimeout(timeout);
  }, [volume]);

  return (
    <View style={{ borderWidth: 8, borderRadius: 16, borderColor: muted ? 'black' : 'white' }}>
      <Rescaler style={{ rescaleMode: 'fill' }}>
        <InputStream inputId={inputId} volume={volume} muted={muted} />
      </Rescaler>
      <View style={{ bottom: 10, left: 10, height: 40, padding: 20 }}>
        <Text style={{ fontSize: 40 }}>
          Input ID: {inputId}, volume: {volume.toFixed(2)} {muted ? 'muted' : 'live'}
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
      resolution: { width: 1920, height: 1080 },
    },
    audio: {
      channels: 'stereo',
      encoder: {
        type: 'aac',
      },
    },
  });

  await smelter.registerInput('input_1', {
    type: 'mp4',
    serverPath: path.join(__dirname, '../.assets/BigBuckBunny.mp4'),
  });

  await smelter.registerInput('input_2', {
    type: 'mp4',
    serverPath: path.join(__dirname, '../.assets/ElephantsDream.mp4'),
  });

  await smelter.start();
}
void run();
