import { Mp4Demuxer } from './Mp4Demuxer';
import type {
  ContainerInfo,
  InputStartResult,
  QueuedInputSource,
  VideoFramePayload,
} from '../input';
import type { Logger } from 'pino';
import { InputVideoDecoder } from '../videoDecoder';
import { InputAudioDecoder } from '../audioDecoder';
import { assert } from '../../../utils';
import type { AudioWorkletMessagePort } from '../../../audioWorkletContext/workletApi';

export default class Mp4Source implements QueuedInputSource {
  private fileUrl: string;
  private logger: Logger;
  private messagePort: AudioWorkletMessagePort;
  private videoDecoder?: InputVideoDecoder;
  private audioDecoder?: InputAudioDecoder;
  private metadata?: ContainerInfo;

  public constructor(fileUrl: string, logger: Logger, messagePort: AudioWorkletMessagePort) {
    this.fileUrl = fileUrl;
    this.logger = logger;
    this.messagePort = messagePort;
  }

  public audioWorkletMessagePort(): AudioWorkletMessagePort {
    return this.messagePort;
  }

  public async init(): Promise<void> {
    const resp = await fetch(this.fileUrl);
    const fileData = await resp.arrayBuffer();

    const demuxer = new Mp4Demuxer(fileData, this.logger);
    await demuxer.init();

    this.videoDecoder = new InputVideoDecoder(demuxer, this.logger);
    this.audioDecoder = new InputAudioDecoder(demuxer, this.logger);
    await Promise.all([this.videoDecoder.init(), this.audioDecoder.init()]);

    this.metadata = demuxer.getMetadata();
  }

  public nextFrame(): VideoFramePayload | undefined {
    assert(this.videoDecoder, 'Decoder was not initialized, call init() first.');
    return this.videoDecoder.nextFrame();
  }

  public getMetadata(): InputStartResult {
    return {
      videoDurationMs: this.metadata?.video?.durationMs,
      audioDurationMs: this.metadata?.audio?.durationMs,
    };
  }

  public close(): void {
    assert(this.videoDecoder, 'Decoder was not initialized, call init() first.');
    this.videoDecoder.close();
  }
}
