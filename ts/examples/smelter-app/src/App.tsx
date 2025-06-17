import { View, useInputStreams, InputStream, Tiles, Rescaler } from '@swmansion/smelter';

import { store } from './store';
import { useStore } from 'zustand';

export default function App() {
  return <OutputScene />;
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
  const inputs = useInputStreams();
  return (
    <Tiles transition={{ durationMs: 300 }}>
      {Object.values(inputs).map(input => (
        <Input key={input.inputId} inputId={input.inputId} />
      ))}
    </Tiles>
  );
}

function PrimaryOnLeftLayout() {
  const connectedStreamIds = useStore(store, state => state.connectedStreamIds);
  const firstStreamId = connectedStreamIds[0];
  if (!firstStreamId) {
    return <View />;
  }
  const inputs = useInputStreams();
  return (
    <View style={{ direction: 'row' }}>
      <Rescaler style={{ width: 1500 }}>
        <Input inputId={firstStreamId} />
      </Rescaler>
      <Tiles transition={{ durationMs: 300 }}>
        {Object.values(inputs)
          .filter(input => input.inputId != firstStreamId)
          .map(input => (
            <Input key={input.inputId} inputId={input.inputId} />
          ))}
      </Tiles>
    </View>
  );
}

function PrimaryOnTopLayout() {
  const connectedStreamIds = useStore(store, state => state.connectedStreamIds);
  const firstStreamId = connectedStreamIds[0];
  if (!firstStreamId) {
    return <View />;
  }
  const inputs = useInputStreams();
  return (
    <View style={{ direction: 'column' }}>
      <Rescaler style={{ height: 800 }}>
        <InputStream inputId={firstStreamId} />
      </Rescaler>
      <Tiles transition={{ durationMs: 300 }}>
        {Object.values(inputs)
          .filter(input => input.inputId != firstStreamId)
          .map(input => (
            <InputStream key={input.inputId} inputId={input.inputId} />
          ))}
      </Tiles>
    </View>
  );
}

function SecondaryInCornerLayout() {
  const connectedStreamIds = useStore(store, state => state.connectedStreamIds);
  const firstStreamId = connectedStreamIds[0];
  const secondStreamId = connectedStreamIds[1];
  if (!firstStreamId) {
    return <View />;
  }
  return (
    <View style={{ direction: 'column' }}>
      <Rescaler>
        <Input inputId={firstStreamId} />
      </Rescaler>
      {secondStreamId ? (
        <Rescaler style={{ top: 80, right: 80, width: 640, height: 320 }}>
          <Input inputId={secondStreamId} />
        </Rescaler>
      ) : null}
    </View>
  );
}
