import { View, Text, useInputStreams, InputStream, Rescaler, Mp4 } from '@swmansion/smelter';
import Smelter, { setWasmBundleUrl } from '@swmansion/smelter-web-wasm';
import { useCallback, useEffect, useState } from 'react';
import SmelterCanvas from '../components/SmelterCanvas';

setWasmBundleUrl('/assets/smelter.wasm');

const CLIP_ID = 'clip';

const MP4_LONG = 'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4';
const MP4_RACE = 'https://www.smelter.dev/videos/smelter-commercial.mp4';

export default function CanvasPage() {
  const [smelter, setSmelter] = useState<Smelter>();

  const addClip = useCallback(async () => {
    await smelter?.unregisterInput(CLIP_ID).catch(() => { })
    try {
      await smelter?.registerInput(CLIP_ID, { 'type': 'mp4', 'url': MP4_RACE });
    } catch (err) {
      console.log(err, 'Failed to register camera')
    }
  }, [smelter])


  return (
    <div>
      <div style={{ textAlign: 'left' }}>
        <h3>Example canvas output</h3>
        <button onClick={addClip} style={{ margin: 10 }}>
          Run clip
        </button>
        <p>Canvas: </p>
      </div>
      <div>
        <SmelterCanvas width={1280} height={720} onSmelterCreated={setSmelter}>
          <SmelterComponent />
        </SmelterCanvas>
      </div>
    </div>
  );
}

function SmelterComponent() {
  const inputs = useInputStreams();

  const [lastClipEnd, setLastClipEnd] = useState<number | undefined>(undefined);
  const [showClip, setShowClip] = useState<boolean>(false);

  useEffect(() => {
    const clip = inputs['clip'];
    if ((lastClipEnd ?? 0) < Date.now() && clip?.videoState === 'playing') {
      setLastClipEnd(Date.now() + (clip.videoDurationMs ?? 0))
      setShowClip(true);
      const durationMs = inputs['clip'].videoDurationMs;
      if (durationMs) {
        setTimeout(() => {
          setShowClip(false)
        }, durationMs - 500)
      }
    }
  }, [inputs, lastClipEnd])


  const mainPosition = showClip
    ? { top: 20, left: 20, width: 320, height: 180, borderWidth: 10, borderRadius: 10 }
    : { top: 0, left: 0, width: 1280, height: 720, borderWidth: 0, borderRadius: 0 };

  return (
    <View style={{ backgroundColor: '#302555' }}>
      <Rescaler>
        <InputStream inputId='clip' />
      </Rescaler>
      <Rescaler style={{ ...mainPosition, borderColor: 'white' }} transition={{ durationMs: 500 }}>
        <Mp4 source={MP4_LONG} />
      </Rescaler>
      <View style={{ bottom: 0, left: 0, height: 50, padding: 20, backgroundColor: '#FFFFFF88' }}>
        <Text style={{ color: 'red', fontSize: 50 }}>Example app</Text>
      </View>
    </View>
  )
}
