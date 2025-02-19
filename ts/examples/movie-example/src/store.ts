import { createStore } from 'zustand';

export type State = {
  showCommercial: boolean;
  toggleCommercial: () => void;
};

export const store = createStore<State>(set => ({
  showCommercial: false,
  toggleCommercial: () => {
    set(state => ({ showCommercial: !state.showCommercial }));
  },
}));
