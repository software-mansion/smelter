import type Smelter from '@swmansion/smelter-web-wasm';
import type { OutputsState } from './PlaygroundPage';
import Scene from './Scene';
import SmelterCanvasOutput from '../../components/SmelterCanvasOutput';
import SmelterVideoOutput from '../../components/SmelterVideoOutput';
import { useCanvasOutputStore, useStreamOutputStore } from './state';

type Props = {
  smelter: Smelter;
  outputs: OutputsState;
};

export default function OutputsSection(props: Props) {
  return (
    <div
      style={{ borderWidth: 4, borderRadius: 8, border: 'solid', flex: 1, margin: 8, padding: 16 }}>
      <h2>Output 1 - canvas</h2>
      <p>
        {props.outputs.canvas.enable ? 'Enabled' : 'Disabled'}{' '}
        {props.outputs.canvas.enable && !props.outputs.canvas.audio ? '(Muted)' : ''}
      </p>
      {props.outputs.canvas.enable && (
        <SmelterCanvasOutput
          smelter={props.smelter}
          audio={!!props.outputs.canvas.audio}
          width={1280}
          height={720}>
          <Scene useStore={useCanvasOutputStore} />
        </SmelterCanvasOutput>
      )}
      <h2>Output 2 - stream</h2>
      <p>
        {props.outputs.stream.enable ? 'Enabled' : 'Disabled'}{' '}
        {props.outputs.stream.enable && !props.outputs.stream.audio ? '(Muted)' : ''}
      </p>
      {props.outputs.stream.enable && (
        <SmelterVideoOutput
          controls
          smelter={props.smelter}
          audio={!!props.outputs.stream.audio}
          width={1280}
          height={720}>
          <Scene useStore={useStreamOutputStore} />
        </SmelterVideoOutput>
      )}
    </div>
  );
}
