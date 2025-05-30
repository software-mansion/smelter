import type {
  RegisterMp4Input,
  RegisterMp4Output,
  RegisterRtmpClientOutput,
  RegisterRtpInput,
  RegisterRtpOutput,
  RegisterWhipInput,
  RegisterWhipOutput,
} from '@swmansion/smelter';

export type RegisterOutput =
  | ({ type: 'rtp_stream' } & RegisterRtpOutput)
  | ({ type: 'mp4' } & RegisterMp4Output)
  | ({ type: 'whip' } & RegisterWhipOutput)
  | ({ type: 'rtmp_client' } & RegisterRtmpClientOutput);

export type RegisterInput =
  | ({ type: 'rtp_stream' } & RegisterRtpInput)
  | ({ type: 'mp4' } & RegisterMp4Input)
  | ({ type: 'whip' } & RegisterWhipInput);
