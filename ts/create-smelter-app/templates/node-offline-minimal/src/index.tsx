import { OfflineSmelter } from '@swmansion/smelter-node';
import { View, Text } from '@swmansion/smelter';

function App() {
  return (
    <View
      style={{
        direction: 'column',
        backgroundColor: '#161127',
        paddingLeft: 200,
      }}>
      <View />
      <Text style={{ fontSize: 50, lineHeight: 80 }}>Open index.tsx and get started.</Text>
      <Text style={{ fontSize: 30, lineHeight: 35, width: 1000, wrap: 'word' }}>
        This is an example of an offline processing with Smelter. It's just a very basic scene that
        displays this text.
      </Text>
      <View />
    </View>
  );
}

async function run() {
  const smelter = new OfflineSmelter();
  await smelter.init();
  await smelter.render(
    <App />,
    {
      type: 'mp4',
      serverPath: './output.mp4',
      video: {
        encoder: { type: 'ffmpeg_h264', preset: 'ultrafast' },
        resolution: { width: 1920, height: 1080 },
      },
      audio: {
        encoder: { type: 'aac', channels: 'stereo' },
      },
    },
    5000
  );
}
void run();
