import type { StateCreator } from 'zustand';
import { create } from 'zustand';

export interface OutputStore {
  cameraVolume: number;
  cameraConnected: boolean;
  screenVolume: number;
  screenConnected: boolean;
  mp4WithAudioVolume: number;
  mp4WithAudioConnected: boolean;
  setCameraVolume: (volume: number) => void;
  setScreenVolume: (volume: number) => void;
  setMp4Volume: (volume: number) => void;
  setCameraConnected: (connected: boolean) => void;
  setScreenConnected: (connected: boolean) => void;
  setMp4Connected: (connected: boolean) => void;
}

const storeFn: StateCreator<OutputStore> = set => ({
  cameraVolume: 1,
  cameraConnected: false,
  screenVolume: 1,
  screenConnected: false,
  mp4WithAudioVolume: 1,
  mp4WithAudioConnected: false,
  setCameraVolume: (volume: number) => set({ cameraVolume: volume }),
  setScreenVolume: (volume: number) => set({ screenVolume: volume }),
  setMp4Volume: (volume: number) => set({ mp4WithAudioVolume: volume }),
  setCameraConnected: (connected: boolean) => set({ cameraConnected: connected }),
  setScreenConnected: (connected: boolean) => set({ screenConnected: connected }),
  setMp4Connected: (connected: boolean) => set({ mp4WithAudioConnected: connected }),
});

export const useCanvasOutputStore = create<OutputStore>(storeFn);
export const useStreamOutputStore = create<OutputStore>(storeFn);
export const useWhipOutputStore = create<OutputStore>(storeFn);
