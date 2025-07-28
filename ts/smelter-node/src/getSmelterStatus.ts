import type { SmelterManager } from '@swmansion/smelter-core';

type InstanceConfiguration = {
  apiPort: number;

  outputFramerate: number;
  mixingSampleRate: number;

  aheadOfTimeProcessing: boolean;
  neverDropOutputFrames: boolean;
  runLateScheduledEvents: boolean;

  downloadRoot: string;

  whipWhepServerPort: number;
  whipWhepStunServers: string[];
  whipWhepEnable: boolean;

  webRendererEnable: boolean;
  webRendererEnableGpu: boolean;

  renderingMode: 'gpu_optimized' | 'cpu_optimized' | 'webgl';
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
  configuration: InstanceConfiguration;
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
    configuration: {
      apiPort: status.configuration.api_port,

      outputFramerate: status.configuration.output_framerate,
      mixingSampleRate: status.configuration.mixing_sample_rate,

      aheadOfTimeProcessing: status.configuration.ahead_of_time_processing,
      runLateScheduledEvents: status.configuration.run_late_scheduled_events,
      neverDropOutputFrames: status.configuration.never_drop_output_frames,

      downloadRoot: status.configuration.download_root,

      webRendererEnable: status.configuration.web_renderer_enable,
      webRendererEnableGpu: status.configuration.web_renderer_enable_gpu,

      whipWhepServerPort: status.configuration.whip_whep_server_port,
      whipWhepEnable: status.configuration.whip_whep_enable,
      whipWhepStunServers: status.configuration.whip_whep_stun_servers,

      renderingMode: status.configuration.rendering_mode,
    },
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
