import type Smelter from '@swmansion/smelter-web-wasm';
import type { OutputsState } from './PlaygroundPage';
import type { OutputStore } from './state';
import { useCanvasOutputStore, useStreamOutputStore } from './state';

type OutputProps = {
  outputs: OutputsState;
  setOutputs: (outputs: OutputsState) => void;
  smelter: Smelter;
};

export default function OutputControls(props: OutputProps) {
  const toggleCanvasOutput = async () => {
    props.setOutputs({
      ...props.outputs,
      canvas: {
        ...props.outputs.canvas,
        enable: !props.outputs.canvas.enable,
      },
    });
  };

  const toggleCanvasAudioOutput = async () => {
    props.setOutputs({
      ...props.outputs,
      canvas: {
        ...props.outputs.canvas,
        audio: !props.outputs.canvas.audio,
      },
    });
  };

  const toggleStreamOutput = async () => {
    props.setOutputs({
      ...props.outputs,
      stream: {
        ...props.outputs.stream,
        enable: !props.outputs.stream.enable,
      },
    });
  };

  const toggleStreamAudioOutput = async () => {
    props.setOutputs({
      ...props.outputs,
      stream: {
        ...props.outputs.stream,
        audio: !props.outputs.stream.audio,
      },
    });
  };

  const canvasStore = useCanvasOutputStore();
  const streamStore = useStreamOutputStore();

  return (
    <div>
      <h3>Output 1 - canvas</h3>
      <button style={{ margin: 8 }} onClick={toggleCanvasOutput}>
        {props.outputs.canvas.enable ? 'Remove' : 'Add'}
      </button>
      <button style={{ margin: 8 }} onClick={toggleCanvasAudioOutput}>
        {props.outputs.canvas.audio ? 'Disable audio' : 'Enable audio'}
      </button>
      <VolumeSliders store={canvasStore} />
      <h3>Output 2 - stream</h3>
      <button style={{ margin: 8 }} onClick={toggleStreamOutput}>
        {props.outputs.stream.enable ? 'Remove' : 'Add'}
      </button>
      <button style={{ margin: 8 }} onClick={toggleStreamAudioOutput}>
        {props.outputs.stream.audio ? 'Disable audio' : 'Enable audio'}
      </button>
      <VolumeSliders store={streamStore} />
    </div>
  );
}

function VolumeSliders(props: { store: OutputStore }) {
  const { store } = props;
  return (
    <div>
      {store.cameraConnected && (
        <div style={{ flexDirection: 'row', display: 'flex', padding: 8 }}>
          <p>Camera volume:</p>
          <div style={{ flex: 1 }} />
          <input
            type="range"
            min={0}
            max={1}
            step={0.01}
            style={{ width: 200 }}
            onChange={ev => {
              store.setCameraVolume(Number(ev.target.value));
            }}
          />
        </div>
      )}
      {store.screenConnected && (
        <div style={{ flexDirection: 'row', display: 'flex', padding: 8 }}>
          <p>Screen share volume:</p>
          <div style={{ flex: 1 }} />
          <input
            type="range"
            min={0}
            max={1}
            step={0.01}
            style={{ width: 200 }}
            onChange={ev => {
              store.setScreenVolume(Number(ev.target.value));
            }}
          />
        </div>
      )}
      {store.mp4WithAudioConnected && (
        <div style={{ flexDirection: 'row', display: 'flex', padding: 8 }}>
          <p>MP4 volume:</p>
          <div style={{ flex: 1 }} />
          <input
            type="range"
            min={0}
            max={1}
            step={0.01}
            style={{ width: 200 }}
            onChange={ev => {
              store.setMp4Volume(Number(ev.target.value));
            }}
          />
        </div>
      )}
    </div>
  );
}
