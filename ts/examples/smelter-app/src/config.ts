import type { Outputs } from '@swmansion/smelter';

type Config = {
  logger: {
    level: 'info' | 'warn';
    transport?: {
      target: 'pino-pretty';
    };
  };
  whepBaseUrl: string;
  whipBaseUrl: string;
  h264Decoder: 'ffmpeg_h264' | 'vulkan_h264';
  h264Encoder: Outputs.WhepVideoEncoderOptions;
};

export const config: Config =
  process.env.ENVIRONMENT === 'production'
    ? {
        logger: {
          level: (process.env.SMELTER_DEMO_ROUTER_LOGGER_LEVEL ?? 'warn') as any,
        },
        whepBaseUrl: 'https://puffer.fishjam.io/smelter-demo-whep/whep',
        whipBaseUrl: 'https://puffer.fishjam.io/smelter-demo-whep/whip',
        h264Decoder: 'vulkan_h264',
        h264Encoder: { type: 'vulkan_h264', bitrate: 20_000_000 },
        //h264Encoder: {
        //  type: 'ffmpeg_h264',
        //  preset: 'veryfast',
        //  ffmpegOptions: {
        //    tune: 'zerolatency',
        //    thread_type: 'slice',
        //  },
        //},
      }
    : {
        logger: {
          transport: {
            target: 'pino-pretty',
          },
          level: (process.env.SMELTER_DEMO_ROUTER_LOGGER_LEVEL ?? 'warn') as any,
        },
        whepBaseUrl: 'http://127.0.0.1:9000/whep',
        whipBaseUrl: 'http://127.0.0.1:9000/whip',
        h264Decoder: 'ffmpeg_h264',
        h264Encoder: {
          type: 'ffmpeg_h264',
          preset: 'ultrafast',
          ffmpegOptions: {
            tune: 'zerolatency',
            thread_type: 'slice',
            preset: 'ultrafast',
            bitrate: '20000000',
          },
        },
      };
