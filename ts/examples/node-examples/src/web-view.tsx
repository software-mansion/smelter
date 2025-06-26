import Smelter, { LocallySpawnedInstanceManager } from '@swmansion/smelter-node';
import { Image, View, WebView, Text, Rescaler, Mp4 } from '@swmansion/smelter';
import { ffplayStartRtmpServerAsync } from './utils';
import path from 'path';

const WEBSITE_INSTANCE = 'example_website';

function ExampleApp() {
  return (
    <View style={{ backgroundColor: '#302555', direction: 'column' }}>
      <Rescaler>
        <WebView instanceId={WEBSITE_INSTANCE}>
          <Mp4 id="example_video" source="https://smelter.dev/videos/template-scene-race.mp4" />
          <Image id="example_image" imageId="logo" />
        </WebView>
      </Rescaler>
      <View style={{ backgroundColor: 'white', height: 50, padding: 20 }}>
        <Text style={{ color: 'black', fontSize: 50 }}>Example WebView</Text>
      </View>
    </View>
  );
}

async function run() {
  const smelter = new Smelter(
    new LocallySpawnedInstanceManager({
      enableWebRenderer: true,
      port: 8081,
      executablePath: process.env.SMELTER_PATH,
    })
  );
  await smelter.init();

  await ffplayStartRtmpServerAsync(9002);

  await smelter.registerImage('logo', {
    assetType: 'svg',
    url: 'https://smelter.dev/images/smelter-logo.svg',
  });
  await smelter.registerWebRenderer(WEBSITE_INSTANCE, {
    url: `file://${path.join(__dirname, './web-view.html')}`,
    resolution: { width: 1920, height: 1080 },
    embeddingMethod: 'native_embedding_over_content',
  });
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
  });
  await smelter.start();
}
void run();
