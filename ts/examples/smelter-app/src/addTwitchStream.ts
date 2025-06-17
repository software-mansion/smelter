import { SmelterInstance } from './smelter';
import { store } from './store';
import { spawn } from './utils';

export async function addTwitchStream(streamId: string): Promise<void> {
  let state = store.getState();
  if (state.availableStreams.filter(stream => stream.id == streamId).length === 0) {
    throw new Error(`Unknown streamId: ${streamId}`);
  }

  if (state.connectedStreamIds.filter(id => id === streamId).length > 0) {
    throw new Error('Already connected stream.');
  }
  state.addStream(streamId);

  try {
    const streamlinkOutput = await spawn(
      'streamlink',
      ['--stream-url', `https://www.twitch.tv/${streamId}`, 'best'],
      {
        stdio: 'pipe',
      }
    );
    const hlsPlaylistUrl = streamlinkOutput.stdout.trim();

    await SmelterInstance.registerInput(streamId, {
      type: 'hls',
      url: hlsPlaylistUrl,
    });
  } catch (err: any) {
    console.log(err.stdout, err.stderr, err);
    state.removeStream(streamId);
    throw err;
  }
}
