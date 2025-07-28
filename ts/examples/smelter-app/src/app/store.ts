import type { StoreApi } from 'zustand';
import { createStore } from 'zustand';

export type InputConfig = {
  inputId: string;
  volume: number;
  title: string;
  description: string;
};

export const LayoutValues = [
  'grid',
  'primary-on-left',
  'primary-on-top',
  'secondary-in-corner',
] as const;

export type Layout = 'grid' | 'primary-on-left' | 'primary-on-top' | 'secondary-in-corner';

export type RoomStore = {
  inputs: InputConfig[];
  layout: Layout;
  updateState: (inputs: InputConfig[], layout: Layout) => void;
};

export function createRoomStore(): StoreApi<RoomStore> {
  return createStore<RoomStore>(set => ({
    inputs: [],
    layout: 'grid',
    updateState: (inputs: InputConfig[], layout: Layout) => {
      set(_state => ({ inputs, layout }));
    },
  }));
}
