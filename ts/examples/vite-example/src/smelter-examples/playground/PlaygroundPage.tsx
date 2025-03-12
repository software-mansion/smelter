import { useState } from 'react';
import Controls from './Controls';
import OutputsSection from './OutputsSection';
import { useSmelter } from '../../hooks/useSmelter';

export type InputsState = {
  mp4WithAudio?: boolean;
  mp4Silent?: boolean;
  camera?: boolean;
  screen?: boolean;
};

export type OutputsState = {
  whipStream: {
    enable: boolean;
    audio: boolean;
    url: string;
    token?: string;
  };
  canvas: { enable: boolean; audio: boolean };
  stream: { enable: boolean; audio: boolean };
};

export default function DynamicExample() {
  const smelter = useSmelter();

  const [inputs, setInputs] = useState<InputsState>({});
  const [outputs, setOutputs] = useState<OutputsState>({
    whipStream: {
      enable: false,
      audio: true,
      url: 'https://g.webrtc.live-video.net:4443/v2/offer',
    },
    stream: { enable: false, audio: true },
    canvas: { enable: false, audio: true },
  });

  if (!smelter) {
    return <div />;
  }

  return (
    <div style={{ flexDirection: 'row', display: 'flex', textAlign: 'left' }}>
      <Controls
        smelter={smelter}
        inputs={inputs}
        outputs={outputs}
        setInputs={setInputs}
        setOutputs={setOutputs}
      />
      <OutputsSection smelter={smelter} outputs={outputs} />
    </div>
  );
}
