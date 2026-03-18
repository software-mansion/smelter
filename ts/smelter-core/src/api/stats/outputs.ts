import type {
  Api,
  WhepOutputStatsReport,
  WhipOutputStatsReport,
  HlsOutputStatsReport,
  Mp4OutputStatsReport,
  RtmpOutputStatsReport,
  RtpOutputStatsReport,
} from '@swmansion/smelter';

export function fromApiWhepOutputStats(report: Api.WhepOutputStatsReport): WhepOutputStatsReport {
  return {
    video: fromApiBitrateTrackStats(report.video),
    audio: fromApiBitrateTrackStats(report.audio),
    connectedPeers: report.connected_peers,
  };
}

export function fromApiWhipOutputStats(report: Api.WhipOutputStatsReport): WhipOutputStatsReport {
  return {
    video: fromApiBitrateTrackStats(report.video),
    audio: fromApiBitrateTrackStats(report.audio),
    isConnected: report.is_connected,
  };
}

export function fromApiHlsOutputStats(report: Api.HlsOutputStatsReport): HlsOutputStatsReport {
  return {
    video: fromApiBitrateTrackStats(report.video),
    audio: fromApiBitrateTrackStats(report.audio),
  };
}

export function fromApiMp4OutputStats(report: Api.Mp4OutputStatsReport): Mp4OutputStatsReport {
  return {
    video: fromApiBitrateTrackStats(report.video),
    audio: fromApiBitrateTrackStats(report.audio),
  };
}

export function fromApiRtmpOutputStats(report: Api.RtmpOutputStatsReport): RtmpOutputStatsReport {
  return {
    video: fromApiBitrateTrackStats(report.video),
    audio: fromApiBitrateTrackStats(report.audio),
  };
}

export function fromApiRtpOutputStats(report: Api.RtpOutputStatsReport): RtpOutputStatsReport {
  return {
    video: fromApiBitrateTrackStats(report.video),
    audio: fromApiBitrateTrackStats(report.audio),
  };
}

function fromApiBitrateTrackStats(report: { bitrate_1_second: number; bitrate_1_minute: number }): {
  bitrate1Second: number;
  bitrate1Minute: number;
} {
  return {
    bitrate1Second: report.bitrate_1_second,
    bitrate1Minute: report.bitrate_1_minute,
  };
}
