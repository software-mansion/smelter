import { eq } from 'drizzle-orm';
import type { LibSQLDatabase } from 'drizzle-orm/libsql';
import { spawn } from 'node:child_process';
import { setTimeout } from 'node:timers/promises';
import { tmpdir } from 'os';
import path from 'path';
import type { Logger } from 'pino';
import { v4 as uuid } from 'uuid';
import { clipJobsTable } from '../db/schema';
import type { ClipJob } from '../entities/job';
import type { SmelterInstance } from '../smelter/smelter';

/** Worker responsible for processing clip jobs. */
export class ClipJobsWorker {
  private readonly outDir: string;

  constructor(
    private readonly db: LibSQLDatabase,
    private readonly smelterInstance: SmelterInstance,
    private readonly logger: Logger
  ) {
    this.outDir = path.join(tmpdir(), '.clipper', 'clips');
  }

  async run() {
    while (true) {
      try {
        const jobs = await this.db
          .select()
          .from(clipJobsTable)
          .where(eq(clipJobsTable.status, 'pending'))
          .orderBy(clipJobsTable.createdAt)
          .limit(10);

        for (const job of jobs) {
          try {
            await this.process(job);
            await this.db
              .update(clipJobsTable)
              .set({ status: 'done' })
              .where(eq(clipJobsTable.id, job.id));
          } catch (err) {
            this.logger.error(err, 'Clip job failed');
            await this.db
              .update(clipJobsTable)
              .set({ status: 'corrupted' })
              .where(eq(clipJobsTable.id, job.id));
          }
        }
      } catch (err) {
        this.logger.fatal(err, 'Failed to process clip jobs batch');
      }

      await setTimeout(5000);
    }
  }

  /** Processes the clip job and returns output .mp4 clip location. */
  private async process(job: ClipJob): Promise<string> {
    const clipOutputFile = path.join(this.outDir, `${uuid()}.mp4`);

    const durationTimestamp = this.formatFFmpegTimestamp(job.duration);
    const positionTimestamp = this.formatFFmpegTimestamp(
      Math.max(
        job.clipTimestamp - this.smelterInstance.egressStartDate!.getTime() - job.duration,
        0
      )
    );

    await new Promise<void>((resolve, reject) => {
      // prettier-ignore
      const ffmpeg = spawn(
        'ffmpeg',
        [
          '-ss', positionTimestamp,
          '-i', this.smelterInstance.playlistFilePath,
          '-t', durationTimestamp,
          '-c:v', 'libx264',
          '-c:a', 'aac',
          clipOutputFile,
        ],
        {
          timeout: job.duration + 5 * 1000,
        }
      );

      ffmpeg.stdout.on('data', data => this.logger.debug(`[ffmpeg stdout]\n${data}`));
      ffmpeg.stderr.on('data', data => this.logger.debug(`[ffmpeg stderr]\n${data}`));

      ffmpeg.on('exit', code => {
        if (code != 0) {
          reject(`ffmpeg failed with exit code ${code}`);
        } else {
          resolve();
        }
      });
    });

    return clipOutputFile;
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
