import { useEffect } from 'react';
import { Image, InputStream, Text, useInputStreams, View } from '@swmansion/smelter';
import NotoSansFont from '../../assets/NotoSans.ttf';
import SmelterCanvasOutput from '../components/SmelterCanvasOutput';
import { useSmelter } from '../hooks/useSmelter';

const MP4_URL =
  'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerEscapes.mp4';

function InputMp4Example() {
  const smelter = useSmelter();
  useEffect(() => {
    if (!smelter) {
      return;
    }
    void (async () => {
      await smelter.registerFont(NotoSansFont);
      await smelter.registerImage('image', {
        assetType: 'svg',
        url: 'https://www.smelter.dev/images/smelter-logo.svg',
        resolution: {
          width: 1000,
          height: 1000,
        },
      } as any);
      await smelter.registerInput('video', { type: 'mp4', url: MP4_URL });
    })();
  }, [smelter]);

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
  const inputs = useInputStreams();
  const inputState = inputs['video']?.videoState;

  if (inputState === 'playing') {
    return (
      <View style={{ width: 1280, height: 720, backgroundColor: 'white' }}>
        <Image source="https://upload.wikimedia.org/wikipedia/commons/7/70/Example.png" />
        <Image imageId="image" />
        <InputStream inputId="video" />
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

export default InputMp4Example;
