import { useEffect, useRef, useState } from 'react';
import Smelter from '@swmansion/smelter-web-wasm';
import { Mp4, Rescaler, Text, View } from '@swmansion/smelter';

function WhipExample() {
  const [smelter, setSmelter] = useState<Smelter | undefined>();
  const previewRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    const smelter = new Smelter({});
    let terminate = false;

    const startPromise = (async () => {
      await smelter.init();
      if (!terminate) {
        setSmelter(smelter);
      }
    })();

    return () => {
      terminate = true;
      void (async () => {
        await startPromise;
        await smelter.terminate();
      })();
    };
  }, []);

  useEffect(() => {
    if (!smelter) {
      return;
    }

    void (async () => {
      const queryParams = new URLSearchParams(window.location.search);
      const streamKey = queryParams.get('twitchKey');
      if (!streamKey) {
        throw new Error('Add "twitchKey" query params with your Twitch stream key.');
      }
      const { stream } = await smelter.registerOutput('output', <Scene />, {
        type: 'whip',
        endpointUrl: 'https://g.webrtc.live-video.net:4443/v2/offer',
        bearerToken: streamKey,
        video: {
          resolution: { width: 1920, height: 1080 },
          maxBitrate: 6_000_000,
        },
        audio: true,
      });

      await smelter.start();

      if (stream && previewRef.current) {
        previewRef.current.srcObject = stream;
        await previewRef.current.play();
      }
    })();
  }, [smelter]);

  return (
    <div className="card">
      <h2>Preview</h2>
      <video ref={previewRef} />
    </div>
  );
}

const MP4_URL =
  'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4';

function Scene() {
  return (
    <View>
      <Rescaler>
        <Mp4 source={MP4_URL} />
      </Rescaler>
      <View style={{ width: 300, height: 40, backgroundColor: '#000000', bottom: 100, left: 520 }}>
        <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>WHIP example</Text>
      </View>
    </View>
  );
}

export default WhipExample;
