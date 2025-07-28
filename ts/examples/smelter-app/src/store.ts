import { createStore } from 'zustand';
import type { TwitchStreamInfo } from './TwitchApi';

export type StreamInfo = {
  type: 'static' | 'live';
  id: string;
  label: string;
  description: string;
  // stream is live
  live: boolean;
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
  markStreamOffline: (streamId: string) => void;
  updateStreamInfo: (stream: TwitchStreamInfo) => void;
  refreshAvailableStreams: (streams: TwitchStreamInfo[]) => void;
};

//fc_25_gameplay.mp4  nba_gameplay.mp4
export const store = createStore<State>(set => ({
  availableStreams: [
    {
      type: 'static',
      id: 'fc_25_gameplay',
      label: '[MP4] FC 25 Gameplay',
      description: '[Static source] EA Sports FC 25 Gameplay',
      live: true,
    },
    {
      type: 'static',
      id: 'nba_gameplay',
      label: '[MP4] NBA 2K25 Gameplay',
      description: '[Static source] NBA 2K25 Gameplay',
      live: true,
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
    set(state => {
      const stream = state.availableStreams.find(info => info.id === streamId);
      const availableStreams =
        stream && stream.type !== 'static' && !stream.live
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
  markStreamOffline: (streamId: string) => {
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
  updateStreamInfo: (update: TwitchStreamInfo) => {
    set(state => ({
      ...state,
      availableStreams: state.availableStreams.map(stream => {
        if (update.streamId === stream.id) {
          return {
            ...stream,
            label: `[Twitch/${update.category}] ${update.displayName}`,
            description: update.title,
            live: true,
          };
        } else {
          return stream;
        }
      }),
    }));
  },
  refreshAvailableStreams: (streams: TwitchStreamInfo[]) => {
    set(state => {
      const newStreams = streams.filter(
        stream => !state.availableStreams.find(info => info.id === stream.streamId)
      );
      const existingStreams = streams.filter(
        stream => !!state.availableStreams.find(info => info.id === stream.streamId)
      );

      // existing streams that will not be removed
      const oldStreamState = state.availableStreams
        .filter(stream => {
          return (
            existingStreams.find(existing => existing.streamId === stream.id) ||
            state.connectedStreamIds.includes(stream.id) ||
            stream.type === 'static'
          );
        })
        .map(stream => {
          const update = existingStreams.find(existing => existing.streamId === stream.id);
          if (update) {
            return {
              ...stream,
              label: `[Twitch/${update.category}] ${update.displayName}`,
              description: update.title,
              live: true,
            };
          } else {
            return { ...stream, pendingDelete: true };
          }
        });
      const newStreamState = newStreams.map(stream => ({
        type: 'live' as const,
        id: stream.streamId,
        label: `[Twitch/${stream.category}] ${stream.displayName}`,
        description: stream.title,
        live: true,
      }));

      const availableStreams = [...oldStreamState, ...newStreamState];

      return {
        ...state,
        availableStreams,
      };
    });
  },
}));
