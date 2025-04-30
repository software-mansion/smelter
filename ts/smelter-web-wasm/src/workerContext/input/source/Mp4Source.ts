import { Mp4Demuxer } from './Mp4Demuxer';
import type {
  AudioDataPayload,
  ContainerInfo,
  InputStartResult,
  QueuedInputSource,
  VideoFramePayload,
} from '../input';
import type { Logger } from 'pino';
import { InputVideoDecoder } from '../videoDecoder';
import { InputAudioDecoder } from '../audioDecoder';
import { assert } from '../../../utils';
import type { WorkloadBalancer } from '../../queue';
import { AudioWorkletMessagePort } from '../../../audioWorkletContext/bridge';

export default class Mp4Source implements QueuedInputSource {
  private data: ArrayBuffer;
  private logger: Logger;
  private messagePort?: AudioWorkletMessagePort;
  private videoDecoder?: InputVideoDecoder;
  private audioDecoder?: InputAudioDecoder;
  private metadata?: ContainerInfo;
  private workloadBalancer: WorkloadBalancer;

  public constructor(
    data: ArrayBuffer,
    logger: Logger,
    workloadBalancer: WorkloadBalancer,
    messagePort?: MessagePort
  ) {
    this.data = data;
    this.logger = logger;
    this.workloadBalancer = workloadBalancer;
    if (messagePort) {
      this.messagePort = new AudioWorkletMessagePort(messagePort, logger);
    }
  }

  public audioWorkletMessagePort(): AudioWorkletMessagePort | undefined {
    return this.messagePort;
  }

  public async init(): Promise<void> {
    const demuxer = new Mp4Demuxer(this.data, this.logger);
    await demuxer.init();
    this.metadata = demuxer.getMetadata();

    this.videoDecoder =
      this.metadata.video && new InputVideoDecoder(demuxer, this.logger, this.workloadBalancer);
    this.audioDecoder =
      this.metadata.audio && new InputAudioDecoder(demuxer, this.logger, this.workloadBalancer);
    await Promise.all([this.videoDecoder?.init(), this.audioDecoder?.init()]);
  }

  public nextFrame(): VideoFramePayload | undefined {
    assert(this.videoDecoder, 'Decoder was not initialized, call init() first.');
    return this.videoDecoder.nextFrame();
  }

  public nextBatch(): AudioDataPayload | undefined {
    assert(this.audioDecoder, 'Decoder was not initialized, call init() first.');
    return this.audioDecoder.nextBatch();
  }

  public getMetadata(): InputStartResult {
    return {
      videoDurationMs: this.metadata?.video?.durationMs,
      audioDurationMs: this.metadata?.audio?.durationMs,
    };
  }

  public close(): void {
    this.videoDecoder?.close();
    this.audioDecoder?.close();
    this.messagePort?.terminate();
  }
}
