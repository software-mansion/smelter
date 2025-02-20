import { OfflineSmelter } from '@swmansion/smelter-node';
import { View, Rescaler, Mp4, useAfterTimestamp, Image } from '@swmansion/smelter';
import { downloadAllAssets } from '../utils';
import path from 'path';

function AppJs() {
  const isGameActive = useAfterTimestamp(4000);
  const isChatActive = useAfterTimestamp(8000);

  const cameraPosition = isGameActive
    ? {
        top: 16,
        left: 16,
        width: 256 * 2,
        height: 180 * 2,
      }
    : {
        top: 1,
        left: 1,
        width: 1920,
        height: 1080,
      };

  return (
    <View>
      {isGameActive && (
        <Rescaler style={{ rescaleMode: 'fill' }}>
          <Mp4 source={path.join(__dirname, 'assets/game2.mp4')} />
        </Rescaler>
      )}

      <Rescaler
        transition={{ durationMs: 650 }}
        style={{ ...cameraPosition, rescaleMode: 'fill', borderRadius: 24 }}>
        <Mp4 source={path.join(__dirname, 'assets/streamer2.mp4')} />
      </Rescaler>

      {isChatActive && (
        <Rescaler style={{ rescaleMode: 'fill' }}>
          <Image source={new URL(Background, import.meta.url).toString()} />
        </Rescaler>
      )}
    </View>
  );
}

async function run() {
  await downloadAllAssets();
  const smelter = new OfflineSmelter();
  await smelter.init();

  await smelter.render(
    <AppJs />,
    {
      type: 'mp4',
      serverPath: path.join(__dirname, '../../.assets/appjss_output.mp4'),

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
        encoder: {
          type: 'aac',
          channels: 'stereo',
        },
      },
    },
    16000
  );
}
void run();
