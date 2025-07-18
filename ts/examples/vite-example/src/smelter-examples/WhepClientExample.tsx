import { useEffect, useState } from 'react';
import { InputStream, Rescaler, View } from '@swmansion/smelter';
import { useSmelter } from '@swmansion/smelter-web-wasm';
import SmelterVideoOutput from '../components/SmelterVideoOutput';

function WhepClientExample() {
  const smelter = useSmelter();

  const [endpointUrl, setEndpointUrl] = useState<string | undefined>();
  const [bearerToken, setBearerToken] = useState<string | undefined>();

  useEffect(() => {
    const queryParams = new URLSearchParams(window.location.search);
    const endpointUrl = queryParams.get('endpointUrl');
    const bearerToken = queryParams.get('bearerToken');
    if (!endpointUrl && !bearerToken) {
      alert('Add both "endpointUrl" and "bearerToken" query parameters for the WHEP connection.');
    } else if (!endpointUrl) {
      alert('Missing "endpointUrl" query parameter. Provide your WHEP endpoint URL.');
    } else if (!bearerToken) {
      alert('Missing "bearerToken" query parameter. Provide your WHEP bearer token.');
    } else {
      setEndpointUrl(endpointUrl);
      setBearerToken(bearerToken);
    }
  }, []);

  useEffect(() => {
    if (!smelter) {
      return;
    }
    void (async () => {
      if (endpointUrl && bearerToken) {
        await smelter.registerInput('input', { type: 'whep', bearerToken, endpointUrl });
      }
    })();
  }, [smelter]);

  if (!smelter) {
    return <div className="card" />;
  }

  return (
    <div>
      <h2>WHEP stream</h2>
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

function Scene() {
  return (
    <View style={{ borderWidth: 5, borderColor: 'white', backgroundColor: 'black' }}>
      <Rescaler style={{ rescaleMode: 'fill' }}>
        <InputStream inputId="input" />
      </Rescaler>
    </View>
  );
}

export default WhepClientExample;
