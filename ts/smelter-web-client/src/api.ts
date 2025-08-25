import type {
  RegisterMp4Input,
  RegisterMp4Output,
  RegisterHlsInput,
  RegisterHlsOutput,
  RegisterRtmpClientOutput,
  RegisterRtpInput,
  RegisterRtpOutput,
  RegisterWhepOutput,
  RegisterWhipInput,
  RegisterWhipOutput,
} from '@swmansion/smelter';

export type RegisterOutput =
  | ({ type: 'rtp_stream' } & RegisterRtpOutput)
  | ({ type: 'mp4' } & RegisterMp4Output)
  | ({ type: 'hls' } & RegisterHlsOutput)
  | ({ type: 'whep' } & RegisterWhepOutput)
  | ({ type: 'whip' } & RegisterWhipOutput)
  | ({ type: 'rtmp_client' } & RegisterRtmpClientOutput);

export type RegisterInput =
  | ({ type: 'rtp_stream' } & RegisterRtpInput)
  | ({ type: 'mp4' } & RegisterMp4Input)
  | ({ type: 'hls' } & RegisterHlsInput)
  | ({ type: 'whip' } & RegisterWhipInput);

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
