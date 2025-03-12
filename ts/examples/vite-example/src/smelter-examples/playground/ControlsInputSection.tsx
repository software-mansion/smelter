import type Smelter from '@swmansion/smelter-web-wasm';
import type { InputsState } from './PlaygroundPage';
import { CAMERA_ID, MP4_AUDIO_ID, MP4_NO_AUDIO_ID, SCREEN_CAPTURE_ID } from './Controls';

type InputProps = {
  inputs: InputsState;
  setInputs: (inputs: InputsState) => void;
  smelter: Smelter;
};

export default function InputControls(props: InputProps) {
  const toggleCamera = async () => {
    if (props.inputs.camera) {
      await props.smelter.unregisterInput(CAMERA_ID);
    } else {
      await props.smelter.unregisterInput(CAMERA_ID).catch(() => {});
      await props.smelter.registerInput(CAMERA_ID, {
        type: 'camera',
      });
    }
    props.setInputs({ ...props.inputs, camera: !props.inputs.camera });
  };

  const toggleScreenCapture = async () => {
    if (props.inputs.screen) {
      await props.smelter.unregisterInput(SCREEN_CAPTURE_ID);
    } else {
      await props.smelter.unregisterInput(SCREEN_CAPTURE_ID).catch(() => {});
      await props.smelter.registerInput(SCREEN_CAPTURE_ID, {
        type: 'screen_capture',
      });
    }
    props.setInputs({ ...props.inputs, screen: !props.inputs.screen });
  };
  const toggleMp4WithAudio = async () => {
    if (props.inputs.mp4WithAudio) {
      await props.smelter.unregisterInput(MP4_AUDIO_ID);
    } else {
      await props.smelter.unregisterInput(MP4_AUDIO_ID).catch(() => {});
      await props.smelter.registerInput(MP4_AUDIO_ID, {
        type: 'mp4',
        url: 'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerEscapes.mp4',
      });
    }
    props.setInputs({ ...props.inputs, mp4WithAudio: !props.inputs.mp4WithAudio });
  };

  const toggleMp4NoAudio = async () => {
    if (props.inputs.mp4Silent) {
      await props.smelter.unregisterInput(MP4_NO_AUDIO_ID);
    } else {
      await props.smelter.unregisterInput(MP4_NO_AUDIO_ID).catch(() => {});
      await props.smelter.registerInput(MP4_NO_AUDIO_ID, {
        type: 'mp4',
        url: 'https://smelter.dev/videos/template-scene-race.mp4',
      });
    }
    props.setInputs({ ...props.inputs, mp4Silent: !props.inputs.mp4Silent });
  };

  return (
    <div>
      <div style={{ flexDirection: 'row', display: 'flex' }}>
        <h3>Camera</h3>
        <div style={{ flex: 1 }} />
        <button style={{ margin: 8 }} onClick={toggleCamera}>
          {props.inputs.camera ? 'Disconnect' : 'Connect'}
        </button>
      </div>
      <div style={{ flexDirection: 'row', display: 'flex' }}>
        <h3>Screen capture</h3>
        <div style={{ flex: 1 }} />
        <button style={{ margin: 8 }} onClick={toggleScreenCapture}>
          {props.inputs.screen ? 'Disconnect' : 'Connect'}
        </button>
      </div>
      <div style={{ flexDirection: 'row', display: 'flex' }}>
        <h3>Mp4 with audio</h3>
        <div style={{ flex: 1 }} />
        <button style={{ margin: 8 }} onClick={toggleMp4WithAudio}>
          {props.inputs.mp4WithAudio ? 'Disconnect' : 'Connect'}
        </button>
      </div>
      <div style={{ flexDirection: 'row', display: 'flex' }}>
        <h3>Mp4 no audio</h3>
        <div style={{ flex: 1 }} />
        <button style={{ margin: 8 }} onClick={toggleMp4NoAudio}>
          {props.inputs.mp4Silent ? 'Disconnect' : 'Connect'}
        </button>
      </div>
    </div>
  );
}
