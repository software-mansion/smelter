import 'dotenv/config';
import { drizzle } from 'drizzle-orm/libsql';
import express, { json, type NextFunction, type Request, type Response } from 'express';
import fs from 'node:fs';
import https from 'node:https';
import { pino } from 'pino';
import { pinoHttp } from 'pino-http';
import { loadConfig } from './config';
import { ClipController } from './controllers/clip.controller';
import { SmelterInstance } from './smelter/smelter';
import { ClipJobsWorker } from './workers/clip-jobs-worker';

async function main() {
  const logger = pino({
    level: process.env.CLIPPER_DEBUG === '1' ? 'debug' : 'info',
  });

  const config = loadConfig(logger);
  const app = express();
  const db = drizzle(config.dbFileName);

  const smelterInstance = new SmelterInstance({ playlistFileName: 'playlist.m3u8' });
  const clipController = new ClipController(db, logger);
  const clipJobsWorker = new ClipJobsWorker(db, smelterInstance, logger);

  await smelterInstance.run();
  void clipJobsWorker.run();

  app.use(pinoHttp());
  app.use(json());

  app.use('/clip', clipController.router());
  app.get('/ping', (_, res) => void res.json({ message: 'pong' }));

  app.use((err: Error, _req: Request, res: Response, next: NextFunction) => {
    if (res.headersSent) {
      return next(err);
    }

    logger.error(err);
    res.status(500);
  });

  const options: https.ServerOptions = {
    cert: fs.readFileSync(config.httpsCertPath),
    key: fs.readFileSync(config.httpsKeyPath),
  };

  const server = https.createServer(options, app);

  server.listen(config.port, config.host, () =>
    logger.info(`Server running at https://${config.host}:${config.port}`)
  );
}

main().catch(err => console.error('failed to start:', err));
