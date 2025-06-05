import { useEffect, useState } from 'react';
import { Mp4, Rescaler, Text, View } from '@swmansion/smelter';
import SmelterWhipOutput from '../components/SmelterWhipOutput';
import { useSmelter } from '@swmansion/smelter-web-wasm';

function WhipExample() {
  const smelter = useSmelter();

  const [bearerToken, setBearerToken] = useState<string | undefined>();

  useEffect(() => {
    const queryParams = new URLSearchParams(window.location.search);
    const streamKey = queryParams.get('twitchKey');
    if (!streamKey) {
      alert('Add "twitchKey" query params with your Twitch stream key.');
      return;
    }
    setBearerToken(streamKey);
  }, []);

  return (
    <div className="card">
      <h2>Preview</h2>
      {smelter && (
        <SmelterWhipOutput
          smelter={smelter}
          endpointUrl="https://g.webrtc.live-video.net:4443/v2/offer"
          bearerToken={bearerToken}
          video={{
            resolution: { width: 1920, height: 1080 },
            maxBitrate: 6_000_000,
          }}
          audio>
          <Scene />
        </SmelterWhipOutput>
      )}
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
