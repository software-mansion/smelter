import { createStore } from 'zustand';
import type { TwitchStreamInfo } from './TwitchApi';

export type StreamInfo = {
  id: string;
  label: string;
  description: string;
  // stream is live
  live: boolean;
  // hls playlist is available locally
  available: boolean;
  // It should be removed, but it is still connected
  pendingDelete?: boolean;
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
  setNotLive: (streamId: string) => void;
  updateInfo: (stream: TwitchStreamInfo) => void;
  refreshAvailableStream: (streams: TwitchStreamInfo[]) => void;
};

export const store = createStore<State>(set => ({
  availableStreams: [],
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
    set(state => {
      const stream = state.availableStreams.find(info => info.id === streamId);
      const availableStreams = stream?.pendingDelete
        ? state.availableStreams.filter(info => info.id !== streamId)
        : state.availableStreams;
      return {
        ...state,
        connectedStreamIds: state.connectedStreamIds.filter(id => streamId !== id),
        availableStreams,
      };
    });
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
  setNotLive: (streamId: string) => {
    set(state => {
      const stream = state.availableStreams.find(info => info.id === streamId);
      if (!stream) {
        return state;
      } else if (state.connectedStreamIds.includes(stream.id)) {
        return {
          ...state,
          availableStreams: state.availableStreams.map(stream => {
            if (streamId === stream.id) {
              return { ...stream, live: false, pendingDelete: true };
            } else {
              return stream;
            }
          }),
        };
      } else {
        return {
          ...state,
          availableStreams: state.availableStreams.filter(stream => stream.id !== streamId),
        };
      }
    });
  },
  updateInfo: (update: TwitchStreamInfo) => {
    set(state => ({
      ...state,
      availableStreams: state.availableStreams.map(stream => {
        if (update.streamId === stream.id) {
          return {
            ...stream,
            label: `[Category: ${update.category}] ${update.displayName}`,
            description: update.title,
            live: true,
          };
        } else {
          return stream;
        }
      }),
    }));
  },
  refreshAvailableStream: (streams: TwitchStreamInfo[]) => {
    set(state => {
      const newStreams = streams.filter(
        stream => !state.availableStreams.find(info => info.id === stream.streamId)
      );
      const existingStreams = streams.filter(stream =>
        state.availableStreams.find(info => info.id === stream.streamId)
      );

      // existing streams that will not be removed
      const oldStreamState = state.availableStreams
        .filter(stream => {
          return (
            existingStreams.find(existing => existing.streamId === stream.id) ||
            state.connectedStreamIds.includes(stream.id)
          );
        })
        .map(stream => {
          const update = existingStreams.find(existing => existing.streamId === stream.id);
          if (update) {
            return {
              ...stream,
              label: `[Category: ${update.category}] ${update.displayName}`,
              description: update.title,
              live: true,
            };
          } else {
            return { ...stream, pendingDelete: true };
          }
        });
      const newStreamState = newStreams.map(stream => ({
        id: stream.streamId,
        label: `[Category: ${stream.category}] ${stream.displayName}`,
        description: stream.title,
        live: true,
        available: false,
      }));

      const availableStreams = [...oldStreamState, ...newStreamState];

      return {
        ...state,
        availableStreams,
      };
    });
  },
}));
