import { createStore } from 'zustand';

export type State = {
  shouldShowInstructions: boolean;
  toggleInstructions: () => void;
};

export const store = createStore<State>(set => ({
  shouldShowInstructions: true,
  toggleInstructions: () =>
    set(state => ({ shouldShowInstructions: !state.shouldShowInstructions })),
}));
