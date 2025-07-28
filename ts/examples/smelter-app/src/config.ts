type Config = {
  logger: {
    level: 'info';
    transport?: {
      target: 'pino-pretty';
    };
  };
  whepBaseUrl: string;
  h264Decoder: 'ffmpeg_h264' | 'vulkan_h264';
};

export const config: Config =
  process.env.NODE_ENV === 'production'
    ? {
        logger: {
          level: 'info',
        },
        whepBaseUrl: 'https://demo.smelter.dev:9000/whep',
        h264Decoder: 'vulkan_h264',
      }
    : {
        logger: {
          transport: {
            target: 'pino-pretty',
          },
          level: 'info',
        },
        whepBaseUrl: 'http://127.0.0.1:9000/whep',
        h264Decoder: 'ffmpeg_h264',
      };
