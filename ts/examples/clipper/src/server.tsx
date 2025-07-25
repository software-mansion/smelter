import 'dotenv/config';
import express, { json } from 'express';
import fs from 'node:fs';
import https from 'node:https';
import { pino } from 'pino';
import { pinoHttp } from 'pino-http';
import * as z from 'zod';
import { ClipController, ClipService } from './clip';
import { SmelterService } from './smelter';

const configSchema = z.object({
  port: z.coerce.number().default(3000),
  host: z.string().default('localhost'),
  httpsCertPath: z.string(),
  httpsKeyPath: z.string(),
  hlsPlaylistPath: z.string(),
  clipsOutDir: z.string(),
});

function loadConfig() {
  const result = configSchema.safeParse({
    port: process.env.CLIPPER_PORT,
    host: process.env.CLIPPER_HOST,
    httpsCertPath: process.env.CLIPPER_HTTPS_CERT_PATH,
    httpsKeyPath: process.env.CLIPPER_HTTPS_KEY_PATH,
    hlsPlaylistPath: process.env.CLIPPER_HLS_PLAYLIST_FILE,
    clipsOutDir: process.env.CLIPPER_CLIPS_OUT_DIR,
  });

  if (result.success) {
    return result.data;
  } else {
    console.log(result.error);
    throw new Error('failed to parse config');
  }
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
