import { useEffect } from 'react';
import { InputStream, Rescaler, Text, View } from '@swmansion/smelter';
import NotoSansFont from '../../assets/NotoSans.ttf';
import SmelterVideoOutput from '../components/SmelterVideoOutput';
import { useSmelter } from '@swmansion/smelter-web-wasm';

function MediaStream() {
  const smelter = useSmelter();

  useEffect(() => {
    if (!smelter) {
      return;
    }
    void (async () => {
      await smelter.registerFont(NotoSansFont);
      try {
        const mediaStream = await navigator.mediaDevices.getDisplayMedia({
          audio: true,
          video: {
            width: { max: 2048 },
            height: { max: 2048 },
          },
        });
        await smelter.registerInput('stream', { type: 'stream', stream: mediaStream });
      } catch (err: any) {
        console.warn('Failed to register mediaStream input', err);
      }
    })();
  }, [smelter]);

  return (
    <div className="card">
      {smelter && (
        <SmelterVideoOutput smelter={smelter} width={1280} height={720}>
          <Scene />
        </SmelterVideoOutput>
      )}
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
        <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>Media stream input</Text>
      </View>
    </View>
  );
}

export default MediaStream;
