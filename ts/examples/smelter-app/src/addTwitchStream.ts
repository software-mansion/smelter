import path from 'path';
import { SmelterInstance } from './smelter';
import { store } from './store';
import type { SpawnPromise } from './utils';
import { sleep, spawn } from './utils';
import { mkdirp, pathExists } from 'fs-extra';

export const SMELTER_WORKDIR = path.join(process.cwd(), 'workingdir');

export const ffmpegPromises: Record<string, SpawnPromise> = {};

export async function addTwitchStream(streamId: string): Promise<void> {
  let state = store.getState();
  if (state.availableStreams.filter(stream => stream.id == streamId).length === 0) {
    throw new Error(`Unknown streamId: ${streamId}`);
  }

  if (state.connectedStreamIds.filter(id => id === streamId).length > 0) {
    throw new Error('Already connected stream.');
  }
  state.addStream(streamId);

  let ffmpegPromise: SpawnPromise | undefined;
  try {
    const streamlinkOutput = await spawn(
      'streamlink',
      ['--stream-url', `https://www.twitch.tv/${streamId}`, 'best'],
      {
        stdio: 'pipe',
      }
    );
    const hlsPlaylistUrl = streamlinkOutput.stdout.trim();

    const transcodedPlaylist = path.join(SMELTER_WORKDIR, streamId, 'index.m3u8');
    await mkdirp(path.dirname(transcodedPlaylist));

    ffmpegPromise = spawn(
      'ffmpeg',
      [
        '-i',
        hlsPlaylistUrl,
        '-c:v',
        process.env.ENVIRONMENT === 'production' ? 'h264_nvenc' : 'libx264',
        '-c:a',
        'aac',
        '-hls_delete_threshold',
        '10',
        '-hls_flags',
        'delete_segments',
        transcodedPlaylist,
      ],
      { stdio: 'ignore' }
    );
    (ffmpegPromise as any).catch((err: any) => {
      console.log('Failed to start FFmpeg hls-to-hls pipeline');
      console.log(err);
    });
    ffmpegPromises[streamId] = ffmpegPromise;

    while (!(await pathExists(transcodedPlaylist))) {
      console.log('waiting');
      await sleep(2000);
    }

    await SmelterInstance.registerInput(streamId, {
      type: 'hls',
      url: transcodedPlaylist,
    });
  } catch (err: any) {
    console.log(err.stdout, err.stderr, err);
    state.removeStream(streamId);
    ffmpegPromise?.child.kill();
    throw err;
  }
}
