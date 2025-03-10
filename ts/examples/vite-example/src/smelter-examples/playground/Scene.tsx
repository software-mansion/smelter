import { InputStream, Text, Rescaler, Tiles, useInputStreams, View } from '@swmansion/smelter';
import type { OutputStore } from './state';
import type { StoreApi, UseBoundStore } from 'zustand';
import { useEffect } from 'react';
import { CAMERA_ID, MP4_AUDIO_ID, SCREEN_CAPTURE_ID } from './Controls';

export default function Scene({ useStore }: { useStore: UseBoundStore<StoreApi<OutputStore>> }) {
  const store = useStore();
  const inputs = useInputStreams();

  useEffect(() => {
    const state = inputs[CAMERA_ID]?.videoState;
    store.setCameraConnected(state === 'playing');
  }, [inputs[CAMERA_ID]?.videoState]);

  useEffect(() => {
    const state = inputs[SCREEN_CAPTURE_ID]?.videoState;
    store.setScreenConnected(state === 'playing');
  }, [inputs[SCREEN_CAPTURE_ID]?.videoState]);

  useEffect(() => {
    const state = inputs[MP4_AUDIO_ID]?.videoState;
    store.setMp4Connected(state === 'playing');
  }, [inputs[MP4_AUDIO_ID]?.videoState]);

  return (
    <View>
      <Tiles style={{ margin: 24 }} transition={{ durationMs: 500 }}>
        {Object.values(inputs).map(input => (
          <InputTile key={input.inputId} volume={volumeForId(store, input.inputId)} {...input} />
        ))}
      </Tiles>
    </View>
  );
}

function volumeForId(store: OutputStore, id: string): number {
  if (id === CAMERA_ID) {
    return store.cameraVolume;
  } else if (id === SCREEN_CAPTURE_ID) {
    return store.screenVolume;
  } else if (id === MP4_AUDIO_ID) {
    return store.mp4WithAudioVolume;
  } else {
    return 0;
  }
}

type InputTileProps = {
  inputId: string;
  videoState?: 'ready' | 'playing' | 'finished';
  audioState?: 'ready' | 'playing' | 'finished';
  offsetMs?: number | null;
  videoDurationMs?: number;
  audioDurationMs?: number;
  volume: number;
};

function InputTile(props: InputTileProps) {
  return (
    <View style={{ direction: 'column', borderWidth: 3, borderRadius: 8, borderColor: 'white' }}>
      <Rescaler>
        <InputStream inputId={props.inputId} volume={props.volume} />
      </Rescaler>
      <View style={{ height: 24, padding: 16, backgroundColor: 'white' }}>
        <Text style={{ fontSize: 24, color: 'black' }}>
          Input: {props.inputId} Volume: {props.volume}
        </Text>
      </View>
    </View>
  );
}
