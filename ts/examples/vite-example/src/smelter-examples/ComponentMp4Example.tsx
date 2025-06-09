import { Mp4, Rescaler, Slide, SlideShow, Text, View } from '@swmansion/smelter';
import SmelterCanvasOutput from '../components/SmelterCanvasOutput';
import { useSmelter } from '@swmansion/smelter-web-wasm';

const FIRST_MP4_URL =
  'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerEscapes.mp4';

const SECOND_MP4_URL =
  'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerBlazes.mp4';

const NO_AUDIO_MP4_URL = 'https://smelter.dev/videos/template-scene-race.mp4';

function ComponentMp4Example() {
  const smelter = useSmelter();
  return (
    <div className="card">
      {smelter && (
        <SmelterCanvasOutput smelter={smelter} width={1280} height={720} audio>
          <Scene />
        </SmelterCanvasOutput>
      )}
    </div>
  );
}

function Scene() {
  return (
    <View>
      <Rescaler>
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
      </Rescaler>
      <View style={{ width: 230, height: 40, backgroundColor: '#000000', bottom: 20, left: 500 }}>
        <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>Playing MP4 file</Text>
      </View>
    </View>
  );
}

export default ComponentMp4Example;
