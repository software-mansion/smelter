import { spawn } from 'child_process';
import { v4 as uuid } from 'uuid';
import type { Logger } from 'pino';
import type { SmelterService } from '../smelter';

type ClipRequest = {
  /** Clip end date. */
  endDate: Date;
  /** Clip duration in ms. */
  clipDuration: number;
};

type ClipServiceOptions = {
  /** Path to HLS playlist file (.m3u8). */
  hlsPlaylistPath: string;
  /** Clips output directory. */
  clipsOutDir: string;
};

export class ClipService {
  constructor(
    private readonly logger: Logger,
    private readonly smelterService: SmelterService,
    private readonly options: ClipServiceOptions
  ) {}

  async clip(req: ClipRequest): Promise<string> {
    if (!this.smelterService.egressStartDate) {
      throw new Error("can't clip, hls egress hasn't started yet.");
    }

    const positionTimestamp =
      req.endDate.getTime() - this.smelterService.egressStartDate.getTime() - req.clipDuration;
    const clipStartTimestamp =
      positionTimestamp > 0 ? this.formatFFmpegTimeDuration(positionTimestamp) : '00:00:00';
    const clipDurationTimestamp = this.formatFFmpegTimeDuration(req.clipDuration);

    const clipId = uuid();
    const clipDest = `${this.options.clipsOutDir}/${clipId}.mp4`;
    const clipLogger = this.logger.child({ clipId });

    await new Promise<void>((resolve, reject) => {
      const ffmpeg = spawn(
        'ffmpeg',
        [
          '-ss',
          clipStartTimestamp,
          '-i',
          this.options.hlsPlaylistPath,
          '-t',
          clipDurationTimestamp,
          '-c:v',
          'libx264',
          '-c:a',
          'aac',
          clipDest,
        ],
        {
          timeout: req.clipDuration + 5 * 1000,
        }
      );

      ffmpeg.stdout.on('data', data => {
        console.log(data);
      });

      ffmpeg.stderr.on('data', data => {
        clipLogger.debug(`ffmpeg error: ${data}`);
      });

      ffmpeg.on('exit', code => {
        if (code != 0) {
          reject(`failed to create clip code=${code}.`);
        } else {
          resolve();
        }
      });
    });

    return clipDest;
  }

  /** Formats timestamp to HH:MM:SS form. */
  private formatFFmpegTimeDuration(timestamp: number): string {
    const hh = this.formatTimeUnit(Math.floor(timestamp / 1000 / 60 / 60));
    const mm = this.formatTimeUnit(Math.floor(timestamp / 1000 / 60) % 60);
    const ss = this.formatTimeUnit(Math.floor(timestamp / 1000) % 60);

    return `${hh}:${mm}:${ss}`;
  }

  private formatTimeUnit(value: number) {
    if (value < 10) {
      return `0${value}`;
    }

    return value;
  }
}
