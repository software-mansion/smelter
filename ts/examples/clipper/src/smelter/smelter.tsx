import Smelter from '@swmansion/smelter-node';
import fs from 'node:fs';
import path from 'node:path';
import type { Logger } from 'pino';
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
  /** Output directory for HLS stream. */
  hlsOutDir: string;
};

export class SmelterInstance {
  /** TODO: This is a hacky way to know when stream started. */
  private _streamStartDate?: Date;
  readonly playlistFilePath: string;

  constructor(config: SmelterInstanceConfig) {
    for (const file of fs.readdirSync(config.hlsOutDir)) {
      fs.unlinkSync(path.join(config.hlsOutDir, file));
    }

    this.playlistFilePath = path.join(config.hlsOutDir, 'playlist.m3u8');
    fs.writeFileSync(this.playlistFilePath, '');
  }

  get streamStartDate() {
    return this._streamStartDate;
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
    this._streamStartDate = new Date();

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
