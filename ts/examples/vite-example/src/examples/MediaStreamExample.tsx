import { useCallback } from 'react';
import type Smelter from '@swmansion/smelter-web-wasm';
import { InputStream, Rescaler, Text, View } from '@swmansion/smelter';
import CompositorCanvas from '../components/SmelterCanvas';
import NotoSansFont from '../../assets/NotoSans.ttf';

function MediaStream() {
  const onCanvasCreated = useCallback(async (compositor: Smelter) => {
    await compositor.registerFont(NotoSansFont);
    try {
      const mediaStream = await navigator.mediaDevices.getDisplayMedia({
        audio: true,
        video: {
          width: { max: 2048 },
          height: { max: 2048 },
        },
      });
      await compositor.registerInput('stream', { type: 'stream', stream: mediaStream });
    } catch (err: any) {
      console.warn('Failed to register mediaStream input', err);
    }
  }, []);

  return (
    <div className="card">
      <CompositorCanvas onCanvasCreated={onCanvasCreated} width={1280} height={720}>
        <Scene />
      </CompositorCanvas>
    </div>
  );
}

function Scene() {
  return (
    <View>
      <Rescaler>
        <InputStream inputId="stream" />
      </Rescaler>
      <View style={{ width: 300, height: 40, backgroundColor: '#000000', bottom: 20, left: 520 }}>
        <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>Camera input</Text>
      </View>
    </View>
  );
}

export default MediaStream;
