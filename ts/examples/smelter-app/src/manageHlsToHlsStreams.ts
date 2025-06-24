import * as fs from 'fs-extra';
import type { SpawnPromise } from './utils';
import { ensureProcessKill, sleep, spawn } from './utils';
import path from 'path';
import type { State } from './store';
import { store } from './store';

export const SMELTER_WORKDIR = path.join(process.cwd(), 'workingdir');

type HlsStreamState = {
  ffmpegPromise: SpawnPromise;
};

const streams: Record<string, HlsStreamState> = {};

export async function manageHlsToHlsStreams() {
  let runAgain = false;
  let blocked = false;
  const onStateChange = async (state: State) => {
    if (blocked) {
      runAgain = true;
      return;
    }
    blocked = true;
    try {
      await monitorStreamsSinglePass(state);
    } catch (err) {
      console.error('Monitor pass thrown an error', err);
    }

    while (runAgain) {
      try {
        runAgain = false;
        await monitorStreamsSinglePass(store.getState());
      } catch (err) {
        console.error('Monitor pass thrown an error', err);
      }
    }
    blocked = false;
  };
  store.subscribe(onStateChange);
  await onStateChange(store.getState());
}

export async function initialCleanup() {
  await fs.mkdirp(SMELTER_WORKDIR);
  let dir = await fs.readdir(SMELTER_WORKDIR);
  for (const subdir of dir) {
    const lockFile = path.join(SMELTER_WORKDIR, subdir, 'pid.lock');
    try {
      if (await fs.pathExists(lockFile)) {
        return;
      }
      const file = await fs.readFile(lockFile, 'utf-8');
      const maybePid = Number(file.trim());
      if (maybePid) {
        await ensureProcessKill(maybePid);
      }
    } catch (err) {
      console.log(`Cleanup in ${subdir}`, err);
    }
  }
  await Promise.allSettled([fs.remove(SMELTER_WORKDIR)]);
  await fs.mkdirp(SMELTER_WORKDIR);
}

// This function assumes it has unique lock on streams object
async function monitorStreamsSinglePass(state: State) {
  const streamsToStart = state.availableStreams.filter(
    availableStream => availableStream.live && !streams[availableStream.id]
  );
  const streamsToStop = Object.entries(streams).filter(([streamId, _hlsState]) => {
    !state.availableStreams.find(stream => stream.id === streamId);
  });

  await Promise.allSettled(
    streamsToStart.map(async streamInfo => {
      try {
        await startStream(streamInfo.id);
      } catch (err) {
        console.log('Failed to start stream', streamInfo, err);
        return;
      }
    })
  );

  streamsToStop.forEach(([streamId, streamState]) => {
    store.getState().setAvailable(streamId, false);
    streamState.ffmpegPromise.child.kill();
  });
  await sleep(1000);
  await Promise.allSettled(
    streamsToStop.map(async ([streamId, streamState]) => {
      try {
        const pid = streamState.ffmpegPromise.child.pid;
        if (pid) {
          await ensureProcessKill(pid);
        }
        await fs.remove(path.join(SMELTER_WORKDIR, streamId));
      } catch (err: any) {
        console.log('Failed to kill process', err);
      }
      delete streams[streamId];
    })
  );
}

async function startStream(streamId: string) {
  const streamlinkOutput = await spawn(
    'streamlink',
    ['--stream-url', `https://www.twitch.tv/${streamId}`, 'best'],
    {
      stdio: 'pipe',
    }
  );
  const hlsPlaylistUrl = streamlinkOutput.stdout.trim();
  const streamDir = path.join(SMELTER_WORKDIR, streamId);
  await fs.mkdirp(streamDir);
  const transcodedPlaylist = path.join(SMELTER_WORKDIR, streamId, 'index.m3u8');
  const ffmpegPromise = spawn(
    'ffmpeg',
    [
      '-i',
      hlsPlaylistUrl,
      ...(process.env.ENVIRONMENT === 'production'
        ? ['-c:v', 'h264_nvenc']
        : ['-c:v', 'libx264', '-preset', 'ultrafast']),
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

  await new Promise<void>((res, rej) => {
    setTimeout(() => {
      res();
    }, 5000);
    ffmpegPromise.then(() => res()).catch(rej);
  });
  await fs.writeFile(path.join(streamDir, 'pid.lock'), `${ffmpegPromise.child.pid}`, 'utf-8');

  streams[streamId] = {
    ffmpegPromise,
  };
  store.getState().setAvailable(streamId, true);
}

export async function waitForStream(streamId: string): Promise<void> {
  const playlistPath = path.join(SMELTER_WORKDIR, streamId, 'index.m3u8');
  while (!(await fs.pathExists(playlistPath))) {
    console.log('waiting');
    await sleep(2000);
  }
}
