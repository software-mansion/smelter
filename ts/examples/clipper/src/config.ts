import * as z from 'zod';

export const configSchema = z.object({
  port: z.coerce.number().default(3000),
  host: z.string().default('localhost'),
  httpsCertPath: z.string(),
  httpsKeyPath: z.string(),
  hlsPlaylistPath: z.string(),
  clipsOutDir: z.string(),
});

export function loadConfig() {
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
    throw new Error('failed to parse config');
  }
}
