import { OfflineSmelter } from '@swmansion/smelter-node';
import { View, Rescaler, Mp4, useAfterTimestamp } from '@swmansion/smelter';
import { downloadAllAssets } from '../utils';
import path from 'path';
import Chat from './Chat';

function AppJs() {
  const isGameActive = useAfterTimestamp(4000);
  const isChatActive = useAfterTimestamp(8000);

  const isChatInactive = useAfterTimestamp(12000);
  const isGameInactive = useAfterTimestamp(16000);

  const cameraPosition =
    isGameActive && !isGameInactive
      ? {
          top: 32,
          left: 32,
          width: 256 * 2,
          height: 180 * 2,
        }
      : {
          top: 0,
          left: 0,
          width: 1920,
          height: 1080,
        };

  return (
    <View>
      {isGameActive && (
        <Rescaler style={{ rescaleMode: 'fill' }}>
          <Mp4 source={path.join(__dirname, 'assets/game4.mp4')} />
        </Rescaler>
      )}
      <Rescaler
        transition={{ durationMs: 650 }}
        style={{ ...cameraPosition, rescaleMode: 'fill', borderRadius: 24 }}>
        <Mp4 source={path.join(__dirname, 'assets/streamer.mp4')} />
      </Rescaler>
      {isChatActive && !isChatInactive && (
        <Rescaler
          transition={{ durationMs: 500 }}
          style={{
            left: 48,
            bottom: 64,
            width: 256 * 2,
            height: 1080 - 2 * 180 - 3 * 48,
            horizontalAlign: 'left',
          }}>
          <Chat width={800} height={900} />
        </Rescaler>
      )}
      )
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
    20000
  );
}
void run();
