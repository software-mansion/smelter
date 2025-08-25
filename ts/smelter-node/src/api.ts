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
  RegisterWhepOutput,
} from '@swmansion/smelter';

export type RegisterOutput =
  | ({ type: 'rtp_stream' } & RegisterRtpOutput)
  | ({ type: 'mp4' } & RegisterMp4Output)
  | ({ type: 'hls' } & RegisterHlsOutput)
  | ({ type: 'whip' } & RegisterWhipOutput)
  | ({ type: 'whep' } & RegisterWhepOutput)
  | ({ type: 'rtmp_client' } & RegisterRtmpClientOutput);

export type RegisterWhepOutputResponse = {
  endpointRoute: string;
};

export type RegisterInput =
  | ({ type: 'rtp_stream' } & RegisterRtpInput)
  | ({ type: 'mp4' } & RegisterMp4Input)
  | ({ type: 'hls' } & RegisterHlsInput)
  | ({ type: 'whip' } & RegisterWhipInput);

export type RegisterMp4InputResponse = {
  videoDurationMs?: number;
  audioDurationMs?: number;
};

export type RegisterWhipInputResponse = {
  bearerToken: string;
  endpointRoute: string;
};
