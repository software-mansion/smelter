import { View } from '@swmansion/smelter';

import type { RoomStore } from './store';
import type { StoreApi } from 'zustand';
import { useStore } from 'zustand';
import { useContext } from 'react';
import { StoreContext } from './store';
import {
  GridLayout,
  PrimaryOnTopLayout,
  PrimaryOnLeftLayout,
  PictureInPictureLayout,
  WrappedLayout,
  WrappedStaticLayout,
  TransitionLayout,
} from './layouts';

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
    <View style={{ backgroundColor: '#000000', padding: 0 }}>
      {layout === 'grid' ? (
        <GridLayout />
      ) : layout === 'primary-on-top' ? (
        <PrimaryOnTopLayout />
      ) : layout === 'primary-on-left' ? (
        <PrimaryOnLeftLayout />
      ) : layout === 'picture-in-picture' ? (
        <PictureInPictureLayout />
      ) : layout === 'wrapped' ? (
        <WrappedLayout />
      ) : layout === 'wrapped-static' ? (
        <WrappedStaticLayout />
      ) : layout === 'transition' ? (
        <TransitionLayout />
      ) : null}
    </View>
  );
}
