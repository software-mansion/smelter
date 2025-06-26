import { Text, View, InputStream, Tiles, Rescaler, useInputStreams } from '@swmansion/smelter';

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
  const stream = useStore(store, state =>
    state.availableStreams.find(stream => stream.id === props.inputId)
  );
  return (
    <Rescaler>
      <View style={{ width: 1920, height: 1080, direction: 'column' }}>
        <Rescaler style={{ rescaleMode: 'fill' }}>
          <InputStream inputId={props.inputId} muted={audioStreamId !== props.inputId} />
        </Rescaler>
        <View
          style={{
            backgroundColor: '#493880',
            height: 90,
            padding: 20,
            borderRadius: 10,
            direction: 'column',
          }}>
          <Text style={{ fontSize: 40, color: 'white' }}>{stream?.label}</Text>
          <View style={{ height: 10 }} />
          <Text style={{ fontSize: 25, color: 'white' }}>{stream?.description}</Text>
        </View>
      </View>
    </Rescaler>
  );
}

function SmallInput(props: { inputId: string }) {
  const audioStreamId = useStore(store, state => state.audioStreamId);
  const stream = useStore(store, state =>
    state.availableStreams.find(stream => stream.id === props.inputId)
  );
  return (
    <Rescaler>
      <View style={{ width: 640, height: 360, direction: 'column' }}>
        <Rescaler style={{ rescaleMode: 'fill' }}>
          <InputStream inputId={props.inputId} muted={audioStreamId !== props.inputId} />
        </Rescaler>
        <View
          style={{
            backgroundColor: '#493880',
            height: 40,
            padding: 20,
            borderRadius: 10,
            direction: 'column',
          }}>
          <Text style={{ fontSize: 30, color: 'white' }}>{stream?.id}</Text>
        </View>
      </View>
    </Rescaler>
  );
}

function GridLayout() {
  const inputs = useVisibleStreams();
  return (
    <Tiles transition={{ durationMs: 300 }} style={{ padding: 20 }}>
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
      <Tiles transition={{ durationMs: 300 }} style={{ padding: 10 }}>
        {Object.values(visibleStreams)
          .filter(input => input.id != firstStream.id)
          .map(input => (
            <SmallInput key={input.id} inputId={input.id} />
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
      <Tiles transition={{ durationMs: 300 }} style={{ padding: 10 }}>
        {Object.values(visibleStreams)
          .filter(input => input.id != firstStream.id)
          .map(input => (
            <SmallInput key={input.id} inputId={input.id} />
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
          <SmallInput inputId={secondStream.id} />
        </Rescaler>
      ) : null}
    </View>
  );
}
