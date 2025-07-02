import type { SmelterManager } from '@swmansion/smelter-core';

type Duration = {
  secs: number;
  nanos: number;
};

type Framerate = {
  num: number;
  den: number;
};

type WebRenderer = {
  enable: boolean;
  enableGpu: boolean;
};

type QueueOptions = {
  defaultBufferDuration: Duration;
  aheadOfTimeProcessing: boolean;
  outputFramerate: Framerate;
  runLateScheduledEvents: boolean;
  neverDropOutputFrames: boolean;
};

type Input = {
  inputId: string;
  inputType: string;
};

type Output = {
  outputId: string;
  outputType: string;
};

export type SmelterStatus = {
  instanceId: string;
  apiPort: number;
  streamFallbackTimeout: Duration;
  downloadRoot: string;
  webRenderer: WebRenderer;
  forceGpu: boolean;
  queueOptions: QueueOptions;
  mixingSampleRate: number;
  requiredWgpuFeatures: string;
  loadSystemFonts: boolean;
  whipWhepServerPort: number;
  startWhipWhep: boolean;
  renderingMode: string;
  stunServers: string[];
  inputs: Input[];
  outputs: Output[];
};

export async function getSmelterStatus(manager: SmelterManager): Promise<SmelterStatus> {
  let status = (await manager.sendRequest({
    method: 'GET',
    route: '/status',
  })) as any;

  return {
    instanceId: status.instance_id,
    apiPort: status.api_port,
    streamFallbackTimeout: status.stream_fallback_timeout,
    downloadRoot: status.download_root,
    webRenderer: {
      enable: status.web_renderer?.enable,
      enableGpu: status.web_renderer?.enable_gpu,
    },
    forceGpu: status.force_gpu,
    queueOptions: {
      defaultBufferDuration: status.queue_options.default_buffer_duration,
      aheadOfTimeProcessing: status.queue_options.ahead_of_time_processing,
      outputFramerate: status.queue_options.output_framerate,
      runLateScheduledEvents: status.queue_options.run_late_scheduled_events,
      neverDropOutputFrames: status.queue_options.never_drop_output_frames,
    },
    mixingSampleRate: status.mixing_sample_rate,
    requiredWgpuFeatures: status.required_wgpu_features,
    loadSystemFonts: status.load_system_fonts,
    whipWhepServerPort: status.whip_whep_server_port,
    startWhipWhep: status.start_whip_whep,
    renderingMode: status.rendering_mode,
    stunServers: status.stun_servers,
    inputs: status.inputs.map((i: any) => ({
      inputId: i.input_id,
      inputType: i.input_type,
    })),
    outputs: status.outputs.map((o: any) => ({
      outputId: o.output_id,
      outputType: o.output_type,
    })),
  };
}
