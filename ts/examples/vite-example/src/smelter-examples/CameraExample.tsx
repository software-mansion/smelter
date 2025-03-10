import { useEffect } from 'react';
import { InputStream, Rescaler, Text, View } from '@swmansion/smelter';
import CompositorCanvas from '../components/SmelterCanvasOutput';
import NotoSansFont from '../../assets/NotoSans.ttf';
import { useSmelter } from '../hooks/useSmelter';

function ScreenCapture() {
  const smelter = useSmelter();

  useEffect(() => {
    if (!smelter) {
      return;
    }
    void (async () => {
      await smelter.registerFont(NotoSansFont);
      try {
        await smelter.registerInput('camera', { type: 'camera' });
      } catch (err: any) {
        console.warn('Failed to register camera input', err);
      }
    })();
  }, [smelter]);

  return (
    <div className="card">
      {smelter && (
        <CompositorCanvas audio smelter={smelter} width={1280} height={720}>
          <Scene />
        </CompositorCanvas>
      )}
    </div>
  );
}

function Scene() {
  return (
    <View>
      <Rescaler>
        <InputStream inputId="camera" />
      </Rescaler>
      <View style={{ width: 300, height: 40, backgroundColor: '#000000', bottom: 20, left: 520 }}>
        <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>Camera input</Text>
      </View>
    </View>
  );
}

export default ScreenCapture;
