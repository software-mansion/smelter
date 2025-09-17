import {
  Text,
  View,
  InputStream,
  Image,
  Tiles,
  Rescaler,
  useInputStreams,
} from '@swmansion/smelter';

import { createRoomStore, type InputConfig as InputConfig, type RoomStore } from './store';
import type { StoreApi } from 'zustand';
import { useStore } from 'zustand';
import { createContext, useContext } from 'react';

export const StoreContext = createContext<StoreApi<RoomStore>>(createRoomStore());

export default function App({ store }: { store: StoreApi<RoomStore> }) {
  return (
    <StoreContext.Provider value={store}>
      <OutputScene />
    </StoreContext.Provider>
  );
}

function OutputScene() {
  const store = useContext(StoreContext);
  const layout = useStore(store, state => state.layout);

  return (
    <View style={{ backgroundColor: '#161127', padding: 0 }}>
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

function Input({ input }: { input: InputConfig }) {
  const streams = useInputStreams();
  const streamState = streams[input.inputId]?.videoState ?? 'finished';
  return (
    <Rescaler style={{ width: 1920, height: 1210 }}>
      <View style={{ width: 1920, height: 1210, direction: 'column' }}>
        {streamState === 'playing' ? (
          <Rescaler style={{ rescaleMode: 'fill' }}>
            <InputStream inputId={input.inputId} volume={input.volume} />
          </Rescaler>
        ) : streamState === 'ready' ? (
          <View style={{ padding: 300 }}>
            <Rescaler style={{ rescaleMode: 'fit' }}>
              <Image imageId="spinner" />
            </Rescaler>
          </View>
        ) : streamState === 'finished' ? (
          <View style={{ padding: 300 }}>
            <Rescaler style={{ rescaleMode: 'fit' }}>
              <Text style={{ fontSize: 600 }}>Stream offline</Text>
            </Rescaler>
          </View>
        ) : (
          <View />
        )}
        <View
          style={{
            backgroundColor: '#493880',
            height: 90,
            padding: 20,
            borderRadius: 10,
            direction: 'column',
          }}>
          <Text style={{ fontSize: 40, color: 'white' }}>{input?.title}</Text>
          <View style={{ height: 10 }} />
          <Text style={{ fontSize: 25, color: 'white' }}>{input?.description}</Text>
        </View>
      </View>
    </Rescaler>
  );
}

function SmallInput({ input }: { input: InputConfig }) {
  return (
    <Rescaler>
      <View style={{ width: 640, height: 360, direction: 'column' }}>
        <Rescaler style={{ rescaleMode: 'fill' }}>
          <InputStream inputId={input.inputId} volume={input.volume} />
        </Rescaler>
        <View
          style={{
            backgroundColor: '#493880',
            height: 40,
            padding: 20,
            borderRadius: 10,
            direction: 'column',
          }}>
          <Text style={{ fontSize: 30, color: 'white' }}>{input.title}</Text>
        </View>
      </View>
    </Rescaler>
  );
}

function GridLayout() {
  const store = useContext(StoreContext);
  const inputs = useStore(store, state => state.inputs);

  return (
    <Tiles transition={{ durationMs: 300 }} style={{ padding: 20, tileAspectRatio: '1920:1210' }}>
      {Object.values(inputs).map(input => (
        <Input key={input.inputId} input={input} />
      ))}
    </Tiles>
  );
}

function PrimaryOnLeftLayout() {
  const store = useContext(StoreContext);
  const inputs = useStore(store, state => state.inputs);
  const firstInput = inputs[0];
  if (!firstInput) {
    return <View />;
  }
  return (
    <View style={{ direction: 'row' }}>
      <Rescaler style={{ width: 1500 }}>
        <Input input={firstInput} />
      </Rescaler>
      <Tiles transition={{ durationMs: 300 }} style={{ padding: 10 }}>
        {Object.values(inputs)
          .filter(input => input.inputId != firstInput.inputId)
          .map(input => (
            <SmallInput key={input.inputId} input={input} />
          ))}
      </Tiles>
    </View>
  );
}

function PrimaryOnTopLayout() {
  const store = useContext(StoreContext);
  const inputs = useStore(store, state => state.inputs);
  const firstInput = inputs[0];
  if (!firstInput) {
    return <View />;
  }

  return (
    <View style={{ direction: 'column' }}>
      <Rescaler style={{ height: 800 }}>
        <Input input={firstInput} />
      </Rescaler>
      <Tiles transition={{ durationMs: 300 }} style={{ padding: 10 }}>
        {Object.values(inputs)
          .filter(input => input.inputId != firstInput.inputId)
          .map(input => (
            <SmallInput key={input.inputId} input={input} />
          ))}
      </Tiles>
    </View>
  );
}

function SecondaryInCornerLayout() {
  const store = useContext(StoreContext);
  const inputs = useStore(store, state => state.inputs);
  const firstInput = inputs[0];
  const secondInput = inputs[1];
  if (!firstInput) {
    return <View />;
  }
  return (
    <View style={{ direction: 'column' }}>
      <Rescaler transition={{ durationMs: 300 }}>
        <Input input={firstInput} />
      </Rescaler>
      {secondInput ? (
        <Rescaler style={{ top: 80, right: 80, width: 640, height: 320 }}>
          <SmallInput input={secondInput} />
        </Rescaler>
      ) : null}
    </View>
  );
}
