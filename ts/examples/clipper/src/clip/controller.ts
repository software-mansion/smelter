import type { Request, Response } from 'express';
import { Router } from 'express';
import type { ClipService } from './service';

export class ClipController {
  constructor(private readonly clipService: ClipService) {}

  router(): Router {
    const router = Router();
    router.get('/', this.clip.bind(this));
    return router;
  }

  async clip(_: Request, res: Response): Promise<void> {
    const clipPath = await this.clipService.clip({
      endDate: new Date(),
      clipDuration: 30 * 1000,
    });

    res.status(201).json({
      clipPath,
    });
  }
}
