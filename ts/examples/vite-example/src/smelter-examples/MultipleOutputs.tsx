import { useEffect } from 'react';
import { InputStream, Rescaler, Text, Tiles, useInputStreams, View } from '@swmansion/smelter';
import NotoSansFont from '../../assets/NotoSans.ttf';
import SmelterCanvasOutput from '../components/SmelterCanvasOutput';
import SmelterVideoOutput from '../components/SmelterVideoOutput';
import { useSmelter } from '@swmansion/smelter-web-wasm';

const FIRST_MP4_URL =
  'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerEscapes.mp4';

const SECOND_MP4_URL =
  'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerBlazes.mp4';

function MultipleOutputs() {
  const smelter = useSmelter();

  useEffect(() => {
    if (!smelter) {
      return;
    }
    void (async () => {
      await smelter.registerFont(NotoSansFont);
      await smelter.registerInput('input_1', { type: 'mp4', url: FIRST_MP4_URL });
      await new Promise<void>(res => setTimeout(() => res(), 3000));
      await smelter.registerInput('input_2', { type: 'mp4', url: SECOND_MP4_URL });
    })();
  }, [smelter]);

  if (!smelter) {
    return <div className="card" />;
  }

  return (
    <div className="card">
      <h2>Inputs</h2>
      <div style={{ flexDirection: 'row', display: 'flex' }}>
        <SmelterCanvasOutput style={{ margin: 20 }} width={600} height={340} smelter={smelter}>
          <Rescaler style={{ borderWidth: 5, borderColor: 'white', rescaleMode: 'fill' }}>
            <InputStream inputId="input_1" muted={true} />
          </Rescaler>
        </SmelterCanvasOutput>
        <SmelterCanvasOutput style={{ margin: 20 }} width={600} height={340} smelter={smelter}>
          <Rescaler style={{ borderWidth: 5, borderColor: 'white', rescaleMode: 'fill' }}>
            <InputStream inputId="input_2" muted={true} />
          </Rescaler>
        </SmelterCanvasOutput>
      </div>

      <h2>Outputs</h2>
      <SmelterVideoOutput
        style={{ margin: 20 }}
        width={1280}
        height={720}
        smelter={smelter}
        audio
        controls>
        <Scene />
      </SmelterVideoOutput>
    </div>
  );
}

function SceneTile(props: { state?: 'ready' | 'playing' | 'finished'; inputId: string }) {
  if (props.state === 'playing') {
    return (
      <View>
        <Rescaler style={{ rescaleMode: 'fill' }}>
          <InputStream inputId={props.inputId} />
        </Rescaler>
        <View style={{ width: 230, height: 40, bottom: 20, left: 20 }}>
          <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>Playing MP4 file</Text>
        </View>
      </View>
    );
  }

  if (props.state === 'finished') {
    return (
      <View style={{ backgroundColor: '#000000' }}>
        <View style={{ width: 530, height: 40, bottom: 20, left: 20 }}>
          <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>Finished playing MP4 file</Text>
        </View>
      </View>
    );
  }

  return (
    <View style={{ backgroundColor: '#000000' }}>
      <View style={{ width: 530, height: 40, bottom: 20, left: 20 }}>
        <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>Loading MP4 file</Text>
      </View>
    </View>
  );
}

function Scene() {
  const inputs = useInputStreams();
  return (
    <View style={{ borderWidth: 5, borderColor: 'white', backgroundColor: 'black' }}>
      <Tiles transition={{ durationMs: 500 }}>
        {Object.values(inputs).map(input => (
          <SceneTile key={input.inputId} state={input.videoState} inputId={input.inputId} />
        ))}
      </Tiles>
    </View>
  );
}

export default MultipleOutputs;
