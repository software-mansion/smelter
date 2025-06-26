import { View, InputStream, Tiles, Rescaler, useInputStreams } from '@swmansion/smelter';

import type { StreamInfo } from './store';
import { store } from './store';
import { useStore } from 'zustand';

export default function App() {
  return <OutputScene />;
}

function useVisibleStreams(): StreamInfo[] {
  const state = useStore(store, state => state);
  const inputs = useInputStreams();

  return state.connectedStreamIds
    .filter(id => !!inputs[id])
    .map(streamId => state.availableStreams.find(info => info.id === streamId))
    .filter(stream => stream?.live && stream.localHlsReady)
    .filter(stream => !!stream);
}

function OutputScene() {
  const layout = useStore(store, state => state.layout);

  return (
    <View style={{ backgroundColor: '#161127', padding: 50 }}>
      {layout === 'grid' ? (
        <GridLayout />
      ) : layout === 'primary-on-top' ? (
        <PrimaryOnTopLayout />
      ) : layout === 'primary-on-left' ? (
        <PrimaryOnLeftLayout />
      ) : layout === 'secondary-in-corner' ? (
        <SecondaryInCornerLayout />
      ) : null}
    </View>
  );
}

function Input(props: { inputId: string }) {
  const audioStreamId = useStore(store, state => state.audioStreamId);
  return <InputStream inputId={props.inputId} muted={audioStreamId !== props.inputId} />;
}

function GridLayout() {
  const inputs = useVisibleStreams();
  return (
    <Tiles transition={{ durationMs: 300 }}>
      {Object.values(inputs).map(input => (
        <Input key={input.id} inputId={input.id} />
      ))}
    </Tiles>
  );
}

function PrimaryOnLeftLayout() {
  const visibleStreams = useVisibleStreams();
  const firstStream = visibleStreams[0];
  if (!firstStream) {
    return <View />;
  }
  return (
    <View style={{ direction: 'row' }}>
      <Rescaler style={{ width: 1500 }}>
        <Input inputId={firstStream.id} />
      </Rescaler>
      <Tiles transition={{ durationMs: 300 }}>
        {Object.values(visibleStreams)
          .filter(input => input.id != firstStream.id)
          .map(input => (
            <Input key={input.id} inputId={input.id} />
          ))}
      </Tiles>
    </View>
  );
}

function PrimaryOnTopLayout() {
  const visibleStreams = useVisibleStreams();
  const firstStream = visibleStreams[0];
  if (!firstStream) {
    return <View />;
  }

  return (
    <View style={{ direction: 'column' }}>
      <Rescaler style={{ height: 800 }}>
        <Input inputId={firstStream.id} />
      </Rescaler>
      <Tiles transition={{ durationMs: 300 }}>
        {Object.values(visibleStreams)
          .filter(input => input.id != firstStream.id)
          .map(input => (
            <Input key={input.id} inputId={input.id} />
          ))}
      </Tiles>
    </View>
  );
}

function SecondaryInCornerLayout() {
  const visibleStreams = useVisibleStreams();
  const firstStream = visibleStreams[0];
  const secondStream = visibleStreams[1];
  if (!firstStream) {
    return <View />;
  }
  return (
    <View style={{ direction: 'column' }}>
      <Rescaler>
        <Input inputId={firstStream.id} />
      </Rescaler>
      {secondStream.id ? (
        <Rescaler style={{ top: 80, right: 80, width: 640, height: 320 }}>
          <Input inputId={secondStream.id} />
        </Rescaler>
      ) : null}
    </View>
  );
}
