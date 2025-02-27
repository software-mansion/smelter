import type { Api } from '@swmansion/smelter';

type AudioInput = {
  source: MediaStreamAudioSourceNode | AudioWorkletNode;
  gain: GainNode;
};

type AudioMixerInput =
  | {
      type: 'mediaStream';
      track: MediaStreamTrack;
    }
  | {
      type: 'worklet';
      node: AudioWorkletNode;
    };

export class AudioMixer {
  private ctx: AudioContext;
  private outputs: Record<string, AudioMixerOutput> = {};
  private inputs: Record<string, AudioMixerInput> = {};

  constructor() {
    this.ctx = new AudioContext();
  }

  public async init(): Promise<void> {
    console.log('init AudioMixerr')
    await this.ctx.audioWorklet.addModule(
      new URL('../esm/runAudioDataProcossor.js', import.meta.url)
    );
    console.log('init AudioMixerr done')
  }

  public addMediaStreamOutput(outputId: string): MediaStreamTrack {
    const outputNode = this.ctx.createMediaStreamDestination();
    const silence = this.ctx.createConstantSource();
    silence.offset.value = 0;
    silence.connect(outputNode);
    silence.start();

    this.outputs[outputId] = new AudioMixerOutput(this.ctx, outputNode);

    return outputNode.stream.getAudioTracks()[0];
  }

  public addPlaybackOutput(outputId: string): void {
    this.outputs[outputId] = new AudioMixerOutput(this.ctx, this.ctx.destination);
  }

  public addMediaStreamInput(inputId: string, track: MediaStreamTrack) {
    this.inputs[inputId] = { type: 'mediaStream', track };
    for (const output of Object.values(this.outputs)) {
      output.addMediaStreamInput(inputId, track);
    }
  }
  public addWorkletInput(inputId: string): MessagePort {
    const node = new AudioWorkletNode(this.ctx, 'audio-data-source');
    this.inputs[inputId] = { type: 'worklet', node };
    for (const output of Object.values(this.outputs)) {
      output.addAudioDataWorkletInput(inputId, node);
    }
    return node.port;
  }

  public removeInput(inputId: string) {
    for (const output of Object.values(this.outputs)) {
      output.removeInput(inputId);
    }
  }

  public removeOutput(outputId: string): void {
    this.outputs[outputId].close();
    delete this.outputs[outputId];
  }

  public update(outputId: string, inputConfig: Api.InputAudio[]) {
    this.outputs[outputId]?.update(inputConfig);
  }

  public async close() {
    await this.ctx.close();
    for (const outputId of Object.keys(this.outputs)) {
      this.removeOutput(outputId);
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

  public addMediaStreamInput(inputId: string, track: MediaStreamTrack) {
    const stream = new MediaStream();
    stream.addTrack(track);
    const source = this.ctx.createMediaStreamSource(stream);
    const gain = this.ctx.createGain();
    source.connect(gain);
    gain.connect(this.outputNode ?? this.ctx.destination);
    this.inputs[inputId] = {
      source,
      gain,
    };
  }

  public addAudioDataWorkletInput(inputId: string, node: AudioWorkletNode) {
    const gain = this.ctx.createGain();

    node.connect(gain);
    gain.connect(this.outputNode ?? this.ctx.destination);
    this.inputs[inputId] = {
      source: node,
      gain,
    };
  }

  public removeInput(inputId: string) {
    this.inputs[inputId]?.source.disconnect();
    this.inputs[inputId]?.gain.disconnect();
    delete this.inputs[inputId];
  }

  public update(inputConfig: Api.InputAudio[]) {
    for (const [inputId, input] of Object.entries(this.inputs)) {
      const config = inputConfig.find(input => input.input_id === inputId);
      input.gain.gain.value = config?.volume || 0;
    }
  }

  public close() {
    for (const inputId of Object.keys(this.inputs)) {
      this.removeInput(inputId);
    }
  }
}
