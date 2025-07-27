import type { Logger } from 'pino';
import * as z from 'zod';

export const configSchema = z.object({
  port: z.coerce.number().default(3000),
  host: z.string().default('localhost'),
  httpsCertPath: z.string(),
  httpsKeyPath: z.string(),
  dbFileName: z.string(),
});

export function loadConfig(logger: Logger) {
  const result = configSchema.safeParse({
    port: process.env.CLIPPER_PORT ?? 3000,
    host: process.env.CLIPPER_HOST ?? 'localhost',
    httpsCertPath: process.env.CLIPPER_HTTPS_CERT_PATH,
    httpsKeyPath: process.env.CLIPPER_HTTPS_KEY_PATH,
    dbFileName: process.env.CLIPPER_DB_FILE_NAME,
  });

  if (result.success) {
    return result.data;
  } else {
    logger.error(result.error);
    throw new Error('failed to parse config');
  }
}
