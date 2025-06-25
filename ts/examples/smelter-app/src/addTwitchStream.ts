import path from 'path';
import { SmelterInstance } from './smelter';
import { store } from './store';
import { waitForStream } from './manageHlsToHlsStreams';

export const SMELTER_WORKDIR = path.join(process.cwd(), 'workingdir');

export async function addTwitchStream(streamId: string): Promise<void> {
  let state = store.getState();
  if (state.availableStreams.filter(stream => stream.id == streamId).length === 0) {
    throw new Error(`Unknown streamId: ${streamId}`);
  }

  if (state.connectedStreamIds.filter(id => id === streamId).length > 0) {
    throw new Error('Already connected stream.');
  }

  try {
    await waitForStream(streamId);

    await SmelterInstance.registerInput(streamId, {
      type: 'hls',
      url: path.join(SMELTER_WORKDIR, streamId, 'index.m3u8'),
    });
    state.addStream(streamId);
  } catch (err: any) {
    console.log(err.body, err);
    throw err;
  }
}
