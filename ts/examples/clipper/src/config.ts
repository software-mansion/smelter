import type { Logger } from 'pino';
import * as z from 'zod';

export const configSchema = z.object({
  port: z.coerce.number().default(3000),
  host: z.string().default('localhost'),
  httpsCertPath: z.string(),
  httpsKeyPath: z.string(),
  dbFileName: z.string(),

  // TODO: Check if directories actually exist.
  hlsOutDir: z.string(),
  clipsOutDir: z.string(),
});

/** Validates configuration. */
export function loadConfig(logger: Logger) {
  const {
    success,
    data: config,
    error,
  } = configSchema.safeParse({
    port: process.env.CLIPPER_PORT ?? 3000,
    host: process.env.CLIPPER_HOST ?? 'localhost',
    httpsCertPath: process.env.CLIPPER_HTTPS_CERT_PATH,
    httpsKeyPath: process.env.CLIPPER_HTTPS_KEY_PATH,
    dbFileName: process.env.CLIPPER_DB_FILE_NAME,
    hlsOutDir: process.env.CLIPPER_HLS_OUT_DIR,
    clipsOutDir: process.env.CLIPPER_CLIPS_OUT_DIR,
  });

  if (success) {
    return config;
  } else {
    logger.error(error);
    throw new Error('failed to parse config');
  }
}

/** Make sure output folders exist. */
export function ensureDirectoryStructure() {}
