import type Smelter from '@swmansion/smelter-web-wasm';
import type { InputsState, OutputsState } from './PlaygroundPage';
import ControlsInputSection from './ControlsInputSection';
import ControlsOutputSection from './ControlsOutputSection';

export const MP4_AUDIO_ID = 'MP4 with audio';
export const MP4_NO_AUDIO_ID = 'MP4 without audio';
export const CAMERA_ID = 'camera';
export const SCREEN_CAPTURE_ID = 'screen capture';

type Props = {
  inputs: InputsState;
  outputs: OutputsState;
  setInputs: (inputs: InputsState) => void;
  setOutputs: (outputs: OutputsState) => void;
  smelter: Smelter;
};

export default function Controls(props: Props) {
  return (
    <div
      style={{
        width: 400,
        borderWidth: 4,
        borderRadius: 8,
        border: 'solid',
        padding: 16,
        margin: 8,
      }}>
      <h2>Inputs</h2>
      <ControlsInputSection
        smelter={props.smelter}
        inputs={props.inputs}
        setInputs={props.setInputs}
      />
      <h2>Outputs</h2>
      <ControlsOutputSection
        smelter={props.smelter}
        outputs={props.outputs}
        setOutputs={props.setOutputs}
      />
    </div>
  );
}
