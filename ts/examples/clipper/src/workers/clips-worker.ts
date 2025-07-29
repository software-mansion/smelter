import { eq } from 'drizzle-orm';
import type { LibSQLDatabase } from 'drizzle-orm/libsql';
import { spawn } from 'node:child_process';
import { setTimeout } from 'node:timers/promises';
import path from 'path';
import type { Logger } from 'pino';
import slugify from 'slugify';
import { v4 as uuid } from 'uuid';
import { clipsTable } from '../db/schema';
import type { Clip } from '../entities/clip';
import type { SmelterInstance } from '../smelter/smelter';

type ClipsWorkerConfig = {
  clipsOutDir: string;
};

/** Worker responsible for processing clip jobs. */
export class ClipsWorker {
  constructor(
    private readonly db: LibSQLDatabase,
    private readonly smelterInstance: SmelterInstance,
    private readonly logger: Logger,
    private readonly config: ClipsWorkerConfig
  ) {}

  async run() {
    while (true) {
      try {
        const pendingClips = await this.db
          .select()
          .from(clipsTable)
          .where(eq(clipsTable.status, 'pending'))
          .orderBy(clipsTable.createdAt)
          .limit(10);

        for (const job of pendingClips) {
          try {
            const filename = await this.process(job);

            await this.db
              .update(clipsTable)
              .set({ status: 'done', filename, updatedAt: '(current_timestamp)' })
              .where(eq(clipsTable.id, job.id));
          } catch (err) {
            this.logger.error(err, 'Clip failed');

            await this.db
              .update(clipsTable)
              .set({ status: 'corrupted', updatedAt: '(current_timestamp)' })
              .where(eq(clipsTable.id, job.id));
          }
        }
      } catch (err) {
        this.logger.fatal(err, 'Failed to process clip jobs batch');
      }

      await setTimeout(5000);
    }
  }

  /** Processes the clip job and returns output .mp4 clip location. */
  private async process(clip: Clip): Promise<string> {
    const clipFilename = `${slugify(clip.name)}-${uuid()}.mp4`;
    const clipOutputFile = path.join(this.config.clipsOutDir, clipFilename);

    const durationTimestamp = this.formatFFmpegTimestamp(clip.duration);
    const positionTimestamp = this.formatFFmpegTimestamp(
      Math.max(
        clip.clipTimestamp - this.smelterInstance.streamStartDate!.getTime() - clip.duration,
        0
      )
    );

    this.logger.debug({ durationTimestamp, positionTimestamp }, 'Processing clip');

    await new Promise<void>((resolve, reject) => {
      // prettier-ignore
      const ffmpeg = spawn(
        'ffmpeg',
        [
          '-live_start_index', '0',
          '-ss', positionTimestamp,
          '-i', this.smelterInstance.playlistFilePath,
          '-t', durationTimestamp,
          '-c:v', 'libx264',
          '-c:a', 'aac',
          clipOutputFile,
        ],
        {
          timeout: clip.duration + 5 * 1000,
        }
      );

      ffmpeg.stdout.on('data', data => this.logger.debug(`[ffmpeg stdout] ${data}`));
      ffmpeg.stderr.on('data', data => this.logger.debug(`[ffmpeg stderr] ${data}`));

      ffmpeg.on('exit', code => {
        if (code != 0) {
          reject(`ffmpeg failed with exit code ${code}`);
        } else {
          resolve();
        }
      });
    });

    return clipFilename;
  }

  private formatFFmpegTimestamp(timestamp: number): string {
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
