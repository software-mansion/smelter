import type { Api } from '@swmansion/smelter';
import type { Logger } from 'pino';

export const MAIN_SAMPLE_RATE = 48_000;

type AudioInput =
  | {
      type: 'stream';
      mediaStreamSourceNode: MediaStreamAudioSourceNode;
      gainNode: GainNode;
    }
  | {
      type: 'worklet';
      workletSourceNode: AudioWorkletNode;
      gainNode: GainNode;
    };

type AudioMixerInput =
  | {
      type: 'stream';
      stream: MediaStream;
    }
  | {
      type: 'worklet';
      // main context
      workletSourceNode: AudioWorkletNode;
    }
  | {
      type: 'worklet-resampler';
      resamplerContext: AudioContext;
      // secondary context
      workletSourceNode: AudioWorkletNode;
      // main context
      mediaStreamDestinationNode: MediaStreamAudioDestinationNode;
    };

async function loadAudioDataProcessorModule(ctx: AudioContext): Promise<void> {
  await ctx.audioWorklet.addModule(new URL('../esm/runAudioDataProcessor.mjs', import.meta.url));
}

export class AudioMixer {
  private logger: Logger;
  private ctx: AudioContext;
  private outputs: Record<string, AudioMixerOutput> = {};
  private inputs: Record<string, AudioMixerInput> = {};

  constructor(logger: Logger) {
    this.ctx = new AudioContext({ sampleRate: MAIN_SAMPLE_RATE });
    this.logger = logger;
  }

  public async init(): Promise<void> {
    await loadAudioDataProcessorModule(this.ctx);
  }

  public addMediaStreamOutput(outputId: string): MediaStreamTrack {
    const outputNode = this.ctx.createMediaStreamDestination();
    const silence = this.ctx.createConstantSource();
    silence.offset.value = 0;
    silence.connect(outputNode);
    silence.start();

    this.outputs[outputId] = new AudioMixerOutput(this.ctx, outputNode);
    for (const [inputId, input] of Object.entries(this.inputs)) {
      if (input.type === 'stream') {
        this.outputs[outputId].addMediaStreamInput(inputId, input.stream);
      } else if (input.type === 'worklet') {
        this.outputs[outputId].addAudioDataWorkletInput(inputId, input.workletSourceNode);
      } else if (input.type === 'worklet-resampler') {
        this.outputs[outputId].addMediaStreamInput(
          inputId,
          input.mediaStreamDestinationNode.stream
        );
      }
    }

    return outputNode.stream.getAudioTracks()[0];
  }

  public addPlaybackOutput(outputId: string): void {
    this.outputs[outputId] = new AudioMixerOutput(this.ctx, this.ctx.destination);
    for (const [inputId, input] of Object.entries(this.inputs)) {
      if (input.type === 'stream') {
        this.outputs[outputId].addMediaStreamInput(inputId, input.stream);
      } else if (input.type === 'worklet') {
        this.outputs[outputId].addAudioDataWorkletInput(inputId, input.workletSourceNode);
      } else if (input.type === 'worklet-resampler') {
        this.outputs[outputId].addMediaStreamInput(
          inputId,
          input.mediaStreamDestinationNode.stream
        );
      }
    }
  }

  public addMediaStreamInput(inputId: string, track: MediaStreamTrack) {
    const stream = new MediaStream();
    stream.addTrack(track);
    this.inputs[inputId] = { type: 'stream', stream };
    for (const output of Object.values(this.outputs)) {
      output.addMediaStreamInput(inputId, stream);
    }
  }

  public addWorkletInput(inputId: string): MessagePort {
    const workletSourceNode = new AudioWorkletNode(this.ctx, 'audio-data-source');
    const logger = this.logger;
    workletSourceNode.onprocessorerror = ev => {
      logger.warn(ev, 'audioWorklet error');
    };
    this.inputs[inputId] = { type: 'worklet', workletSourceNode };
    for (const output of Object.values(this.outputs)) {
      output.addAudioDataWorkletInput(inputId, workletSourceNode);
    }

    return workletSourceNode.port;
  }

  public async addWorkletInputWithResample(
    inputId: string,
    sampleRate: number
  ): Promise<MessagePort> {
    const resamplerContext = new AudioContext({ sampleRate });
    await loadAudioDataProcessorModule(resamplerContext);
    const mediaStreamDestinationNode = resamplerContext.createMediaStreamDestination();
    const workletSourceNode = new AudioWorkletNode(resamplerContext, 'audio-data-source');
    const logger = this.logger;
    workletSourceNode.onprocessorerror = ev => {
      logger.warn(ev, 'audioWorklet error');
    };
    workletSourceNode.connect(mediaStreamDestinationNode);

    this.inputs[inputId] = {
      type: 'worklet-resampler',
      workletSourceNode,
      resamplerContext,
      mediaStreamDestinationNode,
    };
    for (const output of Object.values(this.outputs)) {
      output.addMediaStreamInput(inputId, mediaStreamDestinationNode.stream);
    }

    return workletSourceNode.port;
  }

  public async removeInput(inputId: string): Promise<void> {
    for (const output of Object.values(this.outputs)) {
      output.removeInput(inputId);
    }
    const input = this.inputs[inputId];
    if (input) {
      delete this.inputs[inputId];

      if (input.type === 'worklet') {
        input.workletSourceNode.disconnect();
      } else if (input.type === 'worklet-resampler') {
        await input.resamplerContext.close();
      }
    }
  }

  public removeOutput(outputId: string): void {
    const output = this.outputs[outputId];
    if (output) {
      delete this.outputs[outputId];
      output.close();
    }
  }

  public update(outputId: string, inputConfig: Api.InputAudio[]) {
    this.outputs[outputId]?.update(inputConfig);
  }

  public async close() {
    await this.ctx.close();
    for (const outputId of Object.keys(this.outputs)) {
      this.removeOutput(outputId);
    }
    for (const input of Object.values(this.inputs)) {
      if (input.type === 'worklet-resampler') {
        input.mediaStreamDestinationNode.disconnect();
        await input.resamplerContext.close();
      }
    }
  }
}

export class AudioMixerOutput<OutputNode extends AudioNode = AudioNode> {
  private ctx: AudioContext;
  private inputs: Record<string, AudioInput> = {};
  protected outputNode: OutputNode;

  constructor(ctx: AudioContext, outputNode: OutputNode) {
    this.ctx = ctx;
    this.outputNode = outputNode;
  }

  public addMediaStreamInput(inputId: string, stream: MediaStream) {
    const mediaStreamSourceNode = this.ctx.createMediaStreamSource(stream);
    const gainNode = this.ctx.createGain();
    mediaStreamSourceNode.connect(gainNode);
    gainNode.connect(this.outputNode ?? this.ctx.destination);
    this.inputs[inputId] = {
      type: 'stream',
      mediaStreamSourceNode,
      gainNode,
    };
  }

  public addAudioDataWorkletInput(inputId: string, sourceNode: AudioWorkletNode) {
    const gainNode = this.ctx.createGain();

    sourceNode.connect(gainNode);
    gainNode.connect(this.outputNode ?? this.ctx.destination);
    this.inputs[inputId] = {
      type: 'worklet',
      workletSourceNode: sourceNode,
      gainNode: gainNode,
    };
  }

  public removeInput(inputId: string) {
    const input = this.inputs[inputId];
    delete this.inputs[inputId];
    if (input) {
      if (input.type === 'stream') {
        input.mediaStreamSourceNode.disconnect();
        input.gainNode.disconnect();
      } else if (input.type === 'worklet') {
        // node is shared between outputs, so we can't disconnect
        // everything
        input.workletSourceNode.disconnect(input.gainNode);
        input.gainNode.disconnect();
      }
    }
  }

  public update(inputConfig: Api.InputAudio[]) {
    for (const [inputId, input] of Object.entries(this.inputs)) {
      const config = inputConfig.find(input => input.input_id === inputId);
      input.gainNode.gain.value = config?.volume || 0;
    }
  }

  public close() {
    for (const inputId of Object.keys(this.inputs)) {
      this.removeInput(inputId);
    }
    this.outputNode.disconnect();
  }
}
