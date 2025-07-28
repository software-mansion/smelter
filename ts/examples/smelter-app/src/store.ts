import type { StoreApi } from 'zustand';
import { createStore } from 'zustand';

export type InputOptions = {
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
  inputs: InputOptions[];
  layout: Layout;
  setLayout: (layout: Layout) => void;
  addInput: (newInput: InputOptions) => void;
  updateInput: (update: InputOptions) => void;
  removeInput: (inputId: string) => void;
};

export function createRoomStore(): StoreApi<RoomStore> {
  return createStore<RoomStore>(set => ({
    inputs: [],
    layout: 'grid',
    setLayout: (layout: Layout) => set(state => ({ ...state, layout })),
    addInput: (newInput: InputOptions) =>
      set(state => ({ ...state, inputs: [...state.inputs, newInput] })),
    updateInput: (update: InputOptions) =>
      set(state => ({
        ...state,
        inputs: state.inputs.map(input => (input.inputId === update.inputId ? update : input)),
      })),
    removeInput: (inputId: string) =>
      set(state => ({ ...state, inputs: state.inputs.filter(input => input.inputId !== inputId) })),
  }));
}

//export const store = createStore<RoomStore>(set => ({
//  availableStreams: [
//    {
//      type: 'static',
//      id: 'fc_25_gameplay',
//      label: '[MP4] FC 25 Gameplay',
//      description: '[Static source] EA Sports FC 25 Gameplay',
//      live: true,
//    },
//    {
//      type: 'static',
//      id: 'nba_gameplay',
//      label: '[MP4] NBA 2K25 Gameplay',
//      description: '[Static source] NBA 2K25 Gameplay',
//      live: true,
//    },
//  ],
//  connectedStreamIds: [],
//  layout: 'grid' as const,
//  setLayout: (layout: Layout) => {
//    set(state => ({ ...state, layout }));
//  },
//  addStream: (streamId: string) => {
//    set(state => ({
//      ...state,
//      connectedStreamIds: [...state.connectedStreamIds, streamId],
//      availableStreams: state.availableStreams.map(stream => {
//        if (streamId === stream.id) {
//          return { ...stream, connected: true };
//        } else {
//          return stream;
//        }
//      }),
//    }));
//  },
//  removeStream: (streamId: string) => {
//    set(state => {
//      const stream = state.availableStreams.find(info => info.id === streamId);
//      const availableStreams =
//        stream && stream.type !== 'static' && !stream.live
//          ? state.availableStreams.filter(info => info.id !== streamId)
//          : state.availableStreams;
//      return {
//        ...state,
//        connectedStreamIds: state.connectedStreamIds.filter(id => streamId !== id),
//        availableStreams,
//      };
//    });
//  },
//  selectAudioStream: (streamId: string) => {
//    set(state => ({
//      ...state,
//      audioStreamId: streamId,
//    }));
//  },
//  markStreamOffline: (streamId: string) => {
//    set(state => {
//      const stream = state.availableStreams.find(info => info.id === streamId);
//      if (!stream) {
//        return state;
//      } else if (state.connectedStreamIds.includes(stream.id)) {
//        return {
//          ...state,
//          availableStreams: state.availableStreams.map(stream => {
//            if (streamId === stream.id) {
//              return { ...stream, live: false, pendingDelete: true };
//            } else {
//              return stream;
//            }
//          }),
//        };
//      } else {
//        return {
//          ...state,
//          availableStreams: state.availableStreams.filter(stream => stream.id !== streamId),
//        };
//      }
//    });
//  },
//  updateInput: (update: TwitchStreamInfo) => {
//    set(state => ({
//      ...state,
//      availableStreams: state.availableStreams.map(stream => {
//        if (update.streamId === stream.id) {
//          return {
//            ...stream,
//            label: `[Twitch/${update.category}] ${update.displayName}`,
//            description: update.title,
//            live: true,
//          };
//        } else {
//          return stream;
//        }
//      }),
//    }));
//  },
//  refreshAvailableStreams: (streams: TwitchStreamInfo[]) => {
//    set(state => {
//      const newStreams = streams.filter(
//        stream => !state.availableStreams.find(info => info.id === stream.streamId)
//      );
//      const existingStreams = streams.filter(
//        stream => !!state.availableStreams.find(info => info.id === stream.streamId)
//      );
//
//      // existing streams that will not be removed
//      const oldStreamState = state.availableStreams
//        .filter(stream => {
//          return (
//            existingStreams.find(existing => existing.streamId === stream.id) ||
//            state.connectedStreamIds.includes(stream.id) ||
//            stream.type === 'static'
//          );
//        })
//        .map(stream => {
//          const update = existingStreams.find(existing => existing.streamId === stream.id);
//          if (update) {
//            return {
//              ...stream,
//              label: `[Twitch/${update.category}] ${update.displayName}`,
//              description: update.title,
//              live: true,
//            };
//          } else {
//            return { ...stream, pendingDelete: true };
//          }
//        });
//      const newStreamState = newStreams.map(stream => ({
//        type: 'live' as const,
//        id: stream.streamId,
//        label: `[Twitch/${stream.category}] ${stream.displayName}`,
//        description: stream.title,
//        live: true,
//      }));
//
//      const availableStreams = [...oldStreamState, ...newStreamState];
//
//      return {
//        ...state,
//        availableStreams,
//      };
//    });
//  },
//}));
