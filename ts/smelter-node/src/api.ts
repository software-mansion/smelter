import type {
  RegisterMp4Input,
  RegisterMp4Output,
  RegisterHlsInput,
  RegisterHlsOutput,
  RegisterRtmpClientOutput,
  RegisterRtpInput,
  RegisterRtpOutput,
  RegisterWhipInput,
  RegisterWhipOutput,
  RegisterWhepInput,
  RegisterWhepOutput,
} from '@swmansion/smelter';

export type RegisterOutput =
  | ({ type: 'rtp_stream' } & RegisterRtpOutput)
  | ({ type: 'mp4' } & RegisterMp4Output)
  | ({ type: 'hls' } & RegisterHlsOutput)
  | ({ type: 'whip_client' } & RegisterWhipOutput)
  | ({ type: 'whep_server' } & RegisterWhepOutput)
  | ({ type: 'rtmp_client' } & RegisterRtmpClientOutput);

export type RegisterWhepOutputResponse = {
  endpointRoute: string;
};

export type RegisterInput =
  | ({ type: 'rtp_stream' } & RegisterRtpInput)
  | ({ type: 'mp4' } & RegisterMp4Input)
  | ({ type: 'hls' } & RegisterHlsInput)
  | ({ type: 'whip_server' } & RegisterWhipInput)
  | ({ type: 'whep_client' } & RegisterWhepInput);

export type RegisterMp4InputResponse = {
  videoDurationMs?: number;
  audioDurationMs?: number;
};

export type RegisterWhipInputResponse = {
  bearerToken: string;
  endpointRoute: string;
};
