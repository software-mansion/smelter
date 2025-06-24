import { createStore } from 'zustand';

export type StreamInfo = {
  id: string;
  label: string;
  live: boolean;
  available: boolean;
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
  setAvailable: (streamId: string, available: boolean) => void;
  setLive: (streamId: string, available: boolean) => void;
};

export const store = createStore<State>(set => ({
  availableStreams: [
    {
      id: 'alveussanctuary',
      label: 'AlveusSanctuary',
      live: true,
      available: false,
    },
    {
      id: 'ferretsoftware',
      label: 'FerretSoftware',
      live: true,
      available: false,
    },
    {
      id: 'marinemammalrescue',
      label: 'MarineMammalRescue',
      live: true,
      available: false,
    },
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
      availableStreams: state.availableStreams.map(stream => {
        if (streamId === stream.id) {
          return { ...stream, connected: true };
        } else {
          return stream;
        }
      }),
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
  setAvailable: (streamId: string, available: boolean) => {
    set(state => ({
      ...state,
      availableStreams: state.availableStreams.map(stream => {
        if (streamId === stream.id) {
          return { ...stream, available };
        } else {
          return stream;
        }
      }),
    }));
  },
  setLive: (streamId: string, live: boolean) => {
    set(state => ({
      ...state,
      availableStreams: state.availableStreams.map(stream => {
        if (streamId === stream.id) {
          return { ...stream, live };
        } else {
          return stream;
        }
      }),
    }));
  },
}));
