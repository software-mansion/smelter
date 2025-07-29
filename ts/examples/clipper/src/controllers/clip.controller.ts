import { eq, asc } from 'drizzle-orm';
import type { LibSQLDatabase } from 'drizzle-orm/libsql';
import type { Request, Response } from 'express';
import { Router } from 'express';
import type { Logger } from 'pino';
import * as z from 'zod';
import { clipsTable } from '../db/schema';
import { Clip } from '../entities/clip';

const postClipPayloadSchema = z.object({
  name: z.string().nonempty(),
  duration: z.coerce.number().min(30).default(30),
});

export class ClipController {
  constructor(
    private readonly db: LibSQLDatabase,
    private readonly logger: Logger
  ) {}

  router(): Router {
    const router = Router();

    router.get('/', this.getClips.bind(this));
    router.get('/:id', this.getClipJobById.bind(this));
    router.post('/', this.postClipJob.bind(this));

    return router;
  }

  async getClips(_req: Request, res: Response): Promise<void> {
    const clips = (await this.db.select().from(clipsTable).orderBy(asc(clipsTable.createdAt))).map(
      clip => this.getClipPublicFields(clip)
    );

    res.status(200).json(clips);
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
      this.logger.debug(error, 'Failed to parse `id` url param');
      return void res.status(400).contentType('application/problem+json').json({
        type: 'request-error',
        message: 'Invalid clip job id.',
      });
    }

    const [clip] = await this.db.select().from(clipsTable).where(eq(clipsTable.id, id));

    if (!clip) {
      return void res.status(404).contentType('application/problem+json').json({
        type: 'not-found',
        message: 'Clip not found.',
      });
    }

    res.status(200).json(this.getClipPublicFields(clip));
  }

  async postClipJob(req: Request, res: Response): Promise<void> {
    const {
      success: isBodyValid,
      data: body,
      error,
    } = await postClipPayloadSchema.safeParseAsync(req.body);

    if (!isBodyValid) {
      this.logger.debug(error, 'Invalid request body');
      return void res.status(400).contentType('application/problem+json').json({
        type: 'bad-request',
        message: 'Invalid request body',
      });
    }

    const [insertedClip] = await this.db
      .insert(clipsTable)
      .values({
        name: body.name,
        clipTimestamp: new Date().getTime(),
        duration: body.duration * 1000,
      })
      .returning();

    if (!insertedClip) {
      this.logger.error('Inserted job not returned.');
      return void res.status(500).contentType('application/problem+json').json({
        type: 'internal',
        message: 'Internal server error',
      });
    }

    res.status(201).json(this.getClipPublicFields(insertedClip));
  }

  private getClipPublicFields({ id, name, status, filename, createdAt, updatedAt }: Clip) {
    return {
      id,
      name,
      status,
      filename,
      createdAt,
      updatedAt,
    };
  }
}
