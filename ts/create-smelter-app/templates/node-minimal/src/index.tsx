import Smelter from '@swmansion/smelter-node';
import { View, Text } from '@swmansion/smelter';
import { ffplayStartPlayerAsync } from './smelterFfplayHelper';

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
        This example renders static text and sends the output stream via RTMP to the local port
        8001. Generated code includes helpers in smelterFfplayHelper.ts that display the output
        stream using ffplay, make sure to remove them for any real production use.
      </Text>
      <View />
    </View>
  );
}

async function run() {
  const smelter = new Smelter();
  await smelter.init();

  // Display output with `ffplay`.
  await ffplayStartPlayerAsync(8001);

  await smelter.registerOutput('output_1', <App />, {
    type: 'rtmp_client',
    url: 'rtmp://127.0.0.1:8001',
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
  });

  // Connect any additional inputs/images/shader you might need before the start.

  await smelter.start();
}
void run();
