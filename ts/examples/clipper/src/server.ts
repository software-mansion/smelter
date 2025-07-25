import 'dotenv/config';
import express, { json, type NextFunction, type Request, type Response } from 'express';
import fs from 'node:fs';
import https from 'node:https';
import { pino } from 'pino';
import { pinoHttp } from 'pino-http';
import { ClipController, ClipService } from './clip';
import { SmelterService } from './smelter';
import { loadConfig } from './config';

function errorHandler(err: Error, _req: Request, res: Response, next: NextFunction) {
  if (res.headersSent) {
    return next(err);
  }

  res.status(500);
  res.render('error', { error: err });
}

async function main() {
  const logger = pino();
  const config = loadConfig();
  const app = express();

  const smelterService = new SmelterService();
  await smelterService.run();

  const clipService = new ClipService(logger, smelterService, {
    clipsOutDir: config.clipsOutDir,
    hlsPlaylistPath: config.hlsPlaylistPath,
  });

  const clipController = new ClipController(clipService);

  app.use(pinoHttp());
  app.use(json());
  app.use('/clip', clipController.router());
  app.get('/ping', (_, res) => void res.json({ message: 'pong' }));
  app.use(errorHandler);

  const options: https.ServerOptions = {
    key: fs.readFileSync('./certs/localhost-key.pem'),
    cert: fs.readFileSync('./certs/localhost.pem'),
  };

  const port = 3000;
  const host = 'localhost';

  const server = https.createServer(options, app);
  server.listen(port, host, () => logger.info(`Server running at https://${host}:${port}`));
}

main().catch(err => console.error('failed to start:', err));
