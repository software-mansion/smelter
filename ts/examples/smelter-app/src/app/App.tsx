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
  MultiplePicturesLayout,
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
    <View style={{ backgroundColor: '#161127', padding: 0 }}>
      {layout === 'grid' ? (
        <GridLayout />
      ) : layout === 'primary-on-top' ? (
        <PrimaryOnTopLayout />
      ) : layout === 'primary-on-left' ? (
        <PrimaryOnLeftLayout />
      ) : layout === 'picture-in-picture' ? (
        <PictureInPictureLayout />
      ) : layout === 'multiple-pictures' ? (
        <MultiplePicturesLayout />
      ) : layout === 'transition' ? (
        <TransitionLayout />
      ) : null}
    </View>
  );
}
