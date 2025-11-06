import type { StoreApi } from 'zustand';
import { createStore } from 'zustand';
import type { ShaderConfig } from '../shaders/shaders';

export type InputConfig = {
  inputId: string;
  volume: number;
  title: string;
  description: string;
  shaders: ShaderConfig[];
};

export const LayoutValues = [
  'grid',
  'primary-on-left',
  'primary-on-top',
  'picture-in-picture',
] as const;

export type Layout = 'grid' | 'primary-on-left' | 'primary-on-top' | 'picture-in-picture';

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
