import { eq } from 'drizzle-orm';
import type { LibSQLDatabase } from 'drizzle-orm/libsql';
import type { Request, Response } from 'express';
import { Router } from 'express';
import type { Logger } from 'pino';
import * as z from 'zod';
import { clipJobsTable } from '../db/schema';

export class ClipController {
  constructor(
    private readonly db: LibSQLDatabase,
    private readonly logger: Logger
  ) {}

  router(): Router {
    const router = Router();

    router.get('/:id', this.getClipJobById.bind(this));
    router.post('/', this.postClipJob.bind(this));

    return router;
  }

  async getClipJobById(req: Request, res: Response): Promise<void> {
    const {
      success,
      data: id,
      error,
    } = await z.coerce
      .number('Expected a number')
      .min(1, "Id can't be smaller than 1")
      .safeParseAsync(req.params.id);

    if (!success) {
      this.logger.debug(error, 'Failed to parse clip job id url param');
      return void res.status(400).contentType('application/problem+json').json({
        type: 'request-error',
        message: 'Invalid clip job id.',
      });
    }

    const [job] = await this.db.select().from(clipJobsTable).where(eq(clipJobsTable.id, id));

    if (!job) {
      return void res.status(404).contentType('application/problem+json').json({
        type: 'not-found',
        message: 'Job not found.',
      });
    }

    const { status } = job;

    res.status(200).json({
      id: id,
      status: status,
    });
  }

  async postClipJob(_: Request, res: Response): Promise<void> {
    const [insertedJob] = await this.db
      .insert(clipJobsTable)
      .values({
        clipTimestamp: new Date().getTime(),
        duration: 30 * 1000,
      })
      .returning();

    if (!insertedJob) {
      this.logger.error('Inserted job not returned.');
      return void res.status(500).contentType('application/problem+json').json({
        type: 'internal',
        message: 'Internal server error.',
      });
    }

    const { id, status } = insertedJob;

    res.status(201).json({
      id,
      status,
    });
  }
}
