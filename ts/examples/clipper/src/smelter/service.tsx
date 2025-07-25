import Smelter from '@swmansion/smelter-node';
import { ffplayStartRtmpServerAsync } from './utils';
import { Playback } from './scenes/playback';

const RESOLUTION = {
  width: 1920,
  height: 1080,
} as const;

const VIDEO_ENCODER_OPTS = {
  type: 'ffmpeg_h264',
  preset: 'ultrafast',
} as const;

// TODO: Make this comfigurable.
export class SmelterService {
  egressStartDate?: Date;

  async run(): Promise<void> {
    const smelter = new Smelter();
    await smelter.init();

    await smelter.registerInput('in', {
      type: 'whip',
    });

    await smelter.registerOutput('output_hls', <Playback />, {
      type: 'hls',
      serverPath: './.hls/playlist.m3u8',
      video: {
        resolution: RESOLUTION,
        encoder: {
          type: 'ffmpeg_h264',
          preset: 'ultrafast',
        },
      },
    });

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
    this.egressStartDate = new Date();
  }
}
