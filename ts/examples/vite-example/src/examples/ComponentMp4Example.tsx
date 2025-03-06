import { useCallback } from 'react';
import type Smelter from '@swmansion/smelter-web-wasm';
import { Mp4, Slide, SlideShow, Text, View } from '@swmansion/smelter';
import CompositorCanvas from '../components/SmelterCanvas';

const FIRST_MP4_URL =
  'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerEscapes.mp4';

const SECOND_MP4_URL =
  'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerBlazes.mp4';

const NO_AUDIO_MP4_URL = 'https://smelter.dev/videos/template-scene-race.mp4';

function ComponentMp4Example() {
  const onCanvasCreate = useCallback(async (compositor: Smelter) => {
    await compositor.registerFont(
      'https://fonts.gstatic.com/s/notosans/v36/o-0mIpQlx3QUlC5A4PNB6Ryti20_6n1iPHjcz6L1SoM-jCpoiyD9A-9a6Vc.ttf'
    );
  }, []);

  return (
    <div className="card">
      <CompositorCanvas onCanvasCreated={onCanvasCreate} width={1280} height={720}>
        <Scene />
      </CompositorCanvas>
    </div>
  );
}

function Scene() {
  return (
    <View style={{ width: 1280, height: 720 }}>
      <SlideShow>
        <Slide>
          <Mp4 source={FIRST_MP4_URL} />
        </Slide>
        <Slide>
          <Mp4 source={NO_AUDIO_MP4_URL} />
        </Slide>
        <Slide>
          <Mp4 source={SECOND_MP4_URL} />
        </Slide>
      </SlideShow>
      <View style={{ width: 230, height: 40, backgroundColor: '#000000', bottom: 20, left: 500 }}>
        <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>Playing MP4 file</Text>
      </View>
    </View>
  );
}

export default ComponentMp4Example;
