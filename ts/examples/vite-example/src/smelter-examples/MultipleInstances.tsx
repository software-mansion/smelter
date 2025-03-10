import { useEffect } from 'react';
import { InputStream, Text, useInputStreams, View } from '@swmansion/smelter';
import NotoSansFont from '../../assets/NotoSans.ttf';
import { useSmelter } from '../hooks/useSmelter';
import SmelterCanvasOutput from '../components/SmelterCanvasOutput';

const MP4_URL =
  'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerEscapes.mp4';

function MultipleCompositors() {
  const smelter1 = useSmelter();
  const smelter2 = useSmelter();

  useEffect(() => {
    if (!smelter1) {
      return;
    }
    void (async () => {
      await smelter1.registerFont(NotoSansFont);
      await smelter1.registerInput('bunny_video', { type: 'mp4', url: MP4_URL });
    })();
  }, [smelter1]);

  useEffect(() => {
    if (!smelter2) {
      return;
    }
    void (async () => {
      await smelter2.registerFont(NotoSansFont);
      await smelter2.registerInput('bunny_video', { type: 'mp4', url: MP4_URL });
    })();
  }, [smelter2]);

  return (
    <div className="card">
      {smelter1 && (
        <SmelterCanvasOutput smelter={smelter1} width={1280} height={720} audio>
          <Scene />
        </SmelterCanvasOutput>
      )}
      {smelter2 && (
        <SmelterCanvasOutput smelter={smelter2} width={1280} height={720} audio>
          <Scene />
        </SmelterCanvasOutput>
      )}
    </div>
  );
}

function Scene() {
  const inputs = useInputStreams();
  const inputState = inputs['bunny_video']?.videoState;

  if (inputState === 'playing') {
    return (
      <View style={{ width: 1280, height: 720 }}>
        <InputStream inputId="bunny_video" />
        <View style={{ width: 230, height: 40, backgroundColor: '#000000', bottom: 20, left: 500 }}>
          <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>Playing MP4 file</Text>
        </View>
      </View>
    );
  }

  if (inputState === 'finished') {
    return (
      <View style={{ backgroundColor: '#000000' }}>
        <View style={{ width: 530, height: 40, bottom: 340, left: 500 }}>
          <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>Finished playing MP4 file</Text>
        </View>
      </View>
    );
  }

  return (
    <View style={{ backgroundColor: '#000000' }}>
      <View style={{ width: 530, height: 40, bottom: 340, left: 500 }}>
        <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>Loading MP4 file</Text>
      </View>
    </View>
  );
}

export default MultipleCompositors;
