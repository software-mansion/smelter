import { createStore } from 'zustand';

export type StreamInfo = {
  id: string;
  label: string;
};

export const LayoutValues = [
  'grid',
  'primary-on-left',
  'primary-on-top',
  'secondary-in-corner',
] as const;

export type Layout = 'grid' | 'primary-on-left' | 'primary-on-top' | 'secondary-in-corner';

export type State = {
  availableStreams: StreamInfo[];
  connectedStreamIds: string[];
  layout: Layout;
  audioStreamId?: string;
  setLayout: (layout: Layout) => void;
  addStream: (streamId: string) => void;
  removeStream: (streamId: string) => void;
  selectAudioStream: (streamId: string) => void;
};

export const store = createStore<State>(set => ({
  availableStreams: [
    { id: 'alveussanctuary', label: 'AlveusSanctuary' },
    { id: 'ferretsoftware', label: 'FerretSoftware' },
    { id: 'marinemammalrescue', label: 'MarineMammalRescue' },
  ],
  connectedStreamIds: [],
  layout: 'grid' as const,
  setLayout: (layout: Layout) => {
    set(state => ({ ...state, layout }));
  },
  addStream: (streamId: string) => {
    set(state => ({
      ...state,
      connectedStreamIds: [...state.connectedStreamIds, streamId],
    }));
  },
  removeStream: (streamId: string) => {
    set(state => ({
      ...state,
      connectedStreamIds: state.connectedStreamIds.filter(id => streamId !== id),
    }));
  },
  selectAudioStream: (streamId: string) => {
    set(state => ({
      ...state,
      audioStreamId: streamId,
    }));
  },
}));
