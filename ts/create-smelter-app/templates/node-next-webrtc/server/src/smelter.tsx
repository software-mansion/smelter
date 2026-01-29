import Smelter from '@swmansion/smelter-node';
import App from './app/App';

class SmelterManager {
  private instance: Smelter;

  constructor() {
    this.instance = new Smelter();
  }

  public async init() {
    await SmelterInstance['instance'].init();
    await SmelterInstance['instance'].start();
    await SmelterInstance.registerWhipInput();
    await SmelterInstance.registerWhepOutput();
  }

  /**
   * To receive the output stream connect with WHEP client
   * to URL http://localhost:9000/whep/output.
   *
   * More than one viewer can connect to this endpoint at a time.
   */
  public async registerWhepOutput(): Promise<void> {
    await this.instance.registerOutput('output', <App />, {
      type: 'whep_server',
      video: {
        encoder: {
          // if your hardware supports it you can use
          // {
          //   type: 'vulkan_h264',
          // }
          type: 'ffmpeg_h264',
          preset: 'ultrafast',
          ffmpegOptions: {
            tune: 'zerolatency',
            thread_type: 'slice',
          },
        },
        resolution: {
          width: 1920,
          height: 1080,
        },
      },
      audio: {
        encoder: {
          type: 'opus',
        },
      },
    });
  }

  /**
   * To send the input stream connect with WHIP client
   * to URL http://localhost:9000/whip/input.
   */
  public async registerWhipInput(): Promise<void> {
    await this.instance.registerInput('input', {
      type: 'whip_server',
      bearerToken: 'example_token',
      video: {
        decoderPreferences: ['ffmpeg_h264'],
        // if your hardware supports it you can use
        // decoderPreferences: ['vulkan_h264'],
      },
    });
  }
}

export const SmelterInstance = new SmelterManager();
