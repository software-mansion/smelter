import { useEffect } from 'react';
import { InputStream, Rescaler, Text, View } from '@swmansion/smelter';
import NotoSansFont from '../../assets/NotoSans.ttf';
import SmelterCanvasOutput from '../components/SmelterCanvasOutput';
import { useSmelter } from '@swmansion/smelter-web-wasm';

function ScreenCaptureExample() {
  const smelter = useSmelter();

  useEffect(() => {
    if (!smelter) {
      return;
    }
    void (async () => {
      await smelter.registerFont(NotoSansFont);
      try {
        await smelter.registerInput('screen', { type: 'screen_capture' });
      } catch (err: any) {
        console.warn('Failed to register screen capture input', err);
      }
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
  return (
    <View>
      <Rescaler>
        <InputStream inputId="screen" />
      </Rescaler>
      <View style={{ width: 400, height: 40, backgroundColor: '#000000', bottom: 20, left: 520 }}>
        <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>Screen capture example</Text>
      </View>
    </View>
  );
}

export default ScreenCaptureExample;
