import Smelter from '@swmansion/smelter-node';
import fs from 'node:fs';
import path from 'node:path';
import { tmpdir } from 'os';
import { Playback } from './scenes/playback';
import { ffplayStartRtmpServerAsync } from './utils';

const RESOLUTION = {
  width: 1920,
  height: 1080,
} as const;

const VIDEO_ENCODER_OPTS = {
  type: 'ffmpeg_h264',
  preset: 'ultrafast',
} as const;

type SmelterInstanceConfig = {
  playlistFileName: string;
};

export class SmelterInstance {
  readonly playlistFilePath: string;
  egressStartDate?: Date;

  constructor(config: SmelterInstanceConfig) {
    // Make sure the directory structure is valid.
    // TODO: This doesn't look like a place to do this.

    const playlistFileDir = path.join(tmpdir(), '.clipper', 'hls');
    fs.mkdirSync(playlistFileDir, { recursive: true });

    this.playlistFilePath = path.join(playlistFileDir, config.playlistFileName);
    fs.writeFileSync(this.playlistFilePath, '');
  }

  async run(): Promise<void> {
    const smelter = new Smelter();
    await smelter.init();

    await smelter.registerInput('in', {
      type: 'whip',
    });

    await smelter.registerOutput('output_hls', <Playback />, {
      type: 'hls',
      serverPath: this.playlistFilePath,
      video: {
        resolution: RESOLUTION,
        encoder: {
          type: 'ffmpeg_h264',
          preset: 'ultrafast',
        },
      },
    });

    // TODO: This is extremely hacky.
    this.egressStartDate = new Date();

    await ffplayStartRtmpServerAsync(9002);

    await smelter.registerOutput('output_preview', <Playback />, {
      type: 'rtmp_client',
      url: 'rtmp://127.0.0.1:9002',
      video: {
        encoder: VIDEO_ENCODER_OPTS,
        resolution: RESOLUTION,
      },
      audio: {
        channels: 'stereo',
        encoder: {
          type: 'aac',
        },
      },
    });

    await smelter.start();
  }
}
