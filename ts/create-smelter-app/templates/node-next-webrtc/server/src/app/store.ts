import { createStore } from 'zustand';
import type { UpdateLayoutType } from '../routes';

export type State = {
  showInstructions: boolean;
  updateLayout: (layout: UpdateLayoutType) => void;
};

export const store = createStore<State>(set => ({
  showInstructions: true,
  updateLayout: (update: UpdateLayoutType) => set(_state => update),
}));
