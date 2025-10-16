import type {
  RegisterMp4Input,
  RegisterMp4Output,
  RegisterHlsInput,
  RegisterHlsOutput,
  RegisterRtmpClientOutput,
  RegisterRtpInput,
  RegisterRtpOutput,
  RegisterWhepInput,
  RegisterWhepOutput,
  RegisterWhipInput,
  RegisterWhipOutput,
} from '@swmansion/smelter';

export type RegisterOutput =
  | ({ type: 'rtp_stream' } & RegisterRtpOutput)
  | ({ type: 'mp4' } & RegisterMp4Output)
  | ({ type: 'hls' } & RegisterHlsOutput)
  | ({ type: 'whep_server' } & RegisterWhepOutput)
  | ({ type: 'whip_client' } & RegisterWhipOutput)
  | ({ type: 'rtmp_client' } & RegisterRtmpClientOutput);

export type RegisterInput =
  | ({ type: 'rtp_stream' } & RegisterRtpInput)
  | ({ type: 'mp4' } & RegisterMp4Input)
  | ({ type: 'hls' } & RegisterHlsInput)
  | ({ type: 'whip_server' } & RegisterWhipInput)
  | ({ type: 'whep_client' } & RegisterWhepInput);

export type RegisterWhepOutputResponse = {
  endpointRoute: string;
};

export type RegisterMp4InputResponse = {
  videoDurationMs?: number;
  audioDurationMs?: number;
};

export type RegisterWhipInputResponse = {
  bearerToken: string;
  endpointRoute: string;
};
