import type {
  RtpInputStatsReport,
  WhepInputStatsReport,
  WhipInputStatsReport,
  HlsInputStatsReport,
  RtmpInputStatsReport,
  Mp4InputStatsReport,
} from './stats/input.js';
import type {
  WhepOutputStatsReport,
  WhipOutputStatsReport,
  HlsOutputStatsReport,
  Mp4OutputStatsReport,
  RtmpOutputStatsReport,
  RtpOutputStatsReport,
} from './stats/output.js';

export type StatsReport = {
  inputs: Record<string, InputStatsReport>;
  outputs: Record<string, OutputStatsReport>;
};

export type InputStatsReport =
  | {
      rtp: RtpInputStatsReport;
    }
  | {
      whip: WhipInputStatsReport;
    }
  | {
      whep: WhepInputStatsReport;
    }
  | {
      hls: HlsInputStatsReport;
    }
  | {
      rtmp: RtmpInputStatsReport;
    }
  | {
      mp4: Mp4InputStatsReport;
    };

export type OutputStatsReport =
  | {
      whep: WhepOutputStatsReport;
    }
  | {
      whip: WhipOutputStatsReport;
    }
  | {
      hls: HlsOutputStatsReport;
    }
  | {
      mp4: Mp4OutputStatsReport;
    }
  | {
      rtmp: RtmpOutputStatsReport;
    }
  | {
      rtp: RtpOutputStatsReport;
    };
