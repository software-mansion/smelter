import 'dotenv/config';
import { drizzle } from 'drizzle-orm/libsql';
import express, { type NextFunction, type Request, type Response } from 'express';
import fs from 'node:fs';
import https from 'node:https';
import { pino } from 'pino';
import { pinoHttp } from 'pino-http';
import { loadConfig } from './config';
import { ClipController } from './controllers/clip.controller';
import { SmelterInstance } from './smelter/smelter';
import { ClipsWorker } from './workers/clips-worker';
import { Router } from 'express';

async function main() {
  const logger = pino({
    level: process.env.CLIPPER_DEBUG === '1' ? 'debug' : 'info',
  });

  const config = loadConfig(logger);
  const db = drizzle(config.dbFileName);

  const smelterInstance = new SmelterInstance(logger, { hlsOutDir: config.hlsOutDir });
  const clipJobsWorker = new ClipsWorker(db, smelterInstance, logger, {
    clipsOutDir: config.clipsOutDir,
  });

  await smelterInstance.run();
  void clipJobsWorker.run();

  const api = Router();
  api.use('/clips', new ClipController(db, logger).router());

  const app = express();
  app.use(pinoHttp());
  app.use(express.json());
  app.use('/static', express.static(config.clipsOutDir));
  app.use('/client', express.static('./client'));
  app.use('/api/v1', api);

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
