import { useCallback } from 'react';
import type Smelter from '@swmansion/smelter-web-wasm';
import { InputStream, Text, useInputStreams, View } from '@swmansion/smelter';
import CompositorCanvas from '../components/SmelterCanvas';
import NotoSansFont from '../../assets/NotoSans.ttf';

const MP4_URL =
  'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerEscapes.mp4';

function MultipleCompositors() {
  const onCanvasCreate = useCallback(async (compositor: Smelter) => {
    await compositor.registerFont(NotoSansFont);
    await compositor.registerInput('bunny_video', { type: 'mp4', url: MP4_URL });
  }, []);

  return (
    <div className="card">
      <CompositorCanvas onCanvasCreate={onCanvasCreate} width={1280} height={720}>
        <Scene />
      </CompositorCanvas>
      <CompositorCanvas onCanvasCreate={onCanvasCreate} width={1280} height={720}>
        <Scene />
      </CompositorCanvas>
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
