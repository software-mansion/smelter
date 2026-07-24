import type {
  RegisterMp4Input,
  RegisterMp4Output,
  RegisterHlsInput,
  RegisterHlsOutput,
  RegisterRtmpServerInput,
  RegisterRtmpClientOutput,
  RegisterMoqClientOutput,
  RegisterRtpInput,
  RegisterRtpOutput,
  RegisterWhepClientInput,
  RegisterWhepServerOutput,
  RegisterWhipServerInput,
  RegisterWhipClientOutput,
  RegisterMoqServerInput,
  RegisterMoqClientInput,
  RegisterV4l2Input,
} from '@swmansion/smelter';

export type RegisterOutput =
  | ({ type: 'rtp_stream' } & RegisterRtpOutput)
  | ({ type: 'mp4' } & RegisterMp4Output)
  | ({ type: 'hls' } & RegisterHlsOutput)
  | ({ type: 'whep_server' } & RegisterWhepServerOutput)
  | ({ type: 'whip_client' } & RegisterWhipClientOutput)
  | ({ type: 'rtmp_client' } & RegisterRtmpClientOutput)
  | ({ type: 'moq_client' } & RegisterMoqClientOutput);

export type RegisterInput =
  | ({ type: 'rtp_stream' } & RegisterRtpInput)
  | ({ type: 'mp4' } & RegisterMp4Input)
  | ({ type: 'hls' } & RegisterHlsInput)
  | ({ type: 'whip_server' } & RegisterWhipServerInput)
  | ({ type: 'whep_client' } & RegisterWhepClientInput)
  | ({ type: 'rtmp_server' } & RegisterRtmpServerInput)
  | ({ type: 'moq_server' } & RegisterMoqServerInput)
  | ({ type: 'moq_client' } & RegisterMoqClientInput)
  | ({ type: 'v4l2' } & RegisterV4l2Input);

export type RegisterWhepServerOutputResponse = {
  endpointRoute: string;
};

export type RegisterMp4InputResponse = {
  videoDurationMs?: number;
  audioDurationMs?: number;
};

export type RegisterWhipServerInputResponse = {
  bearerToken: string;
  endpointRoute: string;
};
