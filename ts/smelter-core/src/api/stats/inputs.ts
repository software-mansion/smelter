import type {
  Api,
  RtpInputStatsReport,
  RtpJitterBufferStatsReport,
  RtpJitterBufferSlidingWindowStatsReport,
  WhipInputStatsReport,
  WhepInputStatsReport,
  HlsInputStatsReport,
  HlsInputTrackStatsReport,
  HlsInputTrackSlidingWindowStatsReport,
  RtmpInputStatsReport,
  RtmpInputTrackStatsReport,
  Mp4InputStatsReport,
  Mp4InputTrackStatsReport,
} from '@swmansion/smelter';

export function fromApiRtpInputStats(report: Api.RtpInputStatsReport): RtpInputStatsReport {
  return {
    videoRtp: fromApiRtpJitterBufferStats(report.video_rtp),
    audioRtp: fromApiRtpJitterBufferStats(report.audio_rtp),
  };
}

export function fromApiWhipInputStats(report: Api.WhipInputStatsReport): WhipInputStatsReport {
  return {
    videoRtp: fromApiRtpJitterBufferStats(report.video_rtp),
    audioRtp: fromApiRtpJitterBufferStats(report.audio_rtp),
  };
}

export function fromApiWhepInputStats(report: Api.WhepInputStatsReport): WhepInputStatsReport {
  return {
    videoRtp: fromApiRtpJitterBufferStats(report.video_rtp),
    audioRtp: fromApiRtpJitterBufferStats(report.audio_rtp),
  };
}

export function fromApiHlsInputStats(report: Api.HlsInputStatsReport): HlsInputStatsReport {
  return {
    video: fromApiHlsInputTrackStats(report.video),
    audio: fromApiHlsInputTrackStats(report.audio),
    corruptedPacketsReceived: report.corrupted_packets_received,
    corruptedPacketsReceivedLast10Seconds: report.corrupted_packets_received_last_10_seconds,
  };
}

export function fromApiRtmpInputStats(report: Api.RtmpInputStatsReport): RtmpInputStatsReport {
  return {
    video: fromApiBitrateTrackStats(report.video),
    audio: fromApiBitrateTrackStats(report.audio),
  };
}

export function fromApiMp4InputStats(report: Api.Mp4InputStatsReport): Mp4InputStatsReport {
  return {
    video: fromApiBitrateTrackStats(report.video),
    audio: fromApiBitrateTrackStats(report.audio),
  };
}

function fromApiRtpJitterBufferStats(
  report: Api.RtpJitterBufferStatsReport
): RtpJitterBufferStatsReport {
  return {
    packetsLost: report.packets_lost,
    packetsReceived: report.packets_received,
    bitrate1Second: report.bitrate_1_second,
    bitrate1Minute: report.bitrate_1_minute,
    last10Seconds: fromApiRtpJitterBufferSlidingWindowStats(report.last_10_seconds),
  };
}

function fromApiRtpJitterBufferSlidingWindowStats(
  report: Api.RtpJitterBufferSlidingWindowStatsReport
): RtpJitterBufferSlidingWindowStatsReport {
  return {
    packetsLost: report.packets_lost,
    packetsReceived: report.packets_received,
    effectiveBufferAvgSeconds: report.effective_buffer_avg_seconds,
    effectiveBufferMaxSeconds: report.effective_buffer_max_seconds,
    effectiveBufferMinSeconds: report.effective_buffer_min_seconds,
    inputBufferAvgSeconds: report.input_buffer_avg_seconds,
    inputBufferMaxSeconds: report.input_buffer_max_seconds,
    inputBufferMinSeconds: report.input_buffer_min_seconds,
  };
}

function fromApiHlsInputTrackStats(report: Api.HlsInputTrackStatsReport): HlsInputTrackStatsReport {
  return {
    packetsReceived: report.packets_received,
    discontinuitiesDetected: report.discontinuities_detected,
    bitrate1Second: report.bitrate_1_second,
    bitrate1Minute: report.bitrate_1_minute,
    last10Seconds: fromApiHlsInputTrackSlidingWindowStats(report.last_10_seconds),
  };
}

function fromApiHlsInputTrackSlidingWindowStats(
  report: Api.HlsInputTrackSlidingWindowStatsReport
): HlsInputTrackSlidingWindowStatsReport {
  return {
    packetsReceived: report.packets_received,
    discontinuitiesDetected: report.discontinuities_detected,
    effectiveBufferAvgSeconds: report.effective_buffer_avg_seconds,
    effectiveBufferMaxSeconds: report.effective_buffer_max_seconds,
    effectiveBufferMinSeconds: report.effective_buffer_min_seconds,
    inputBufferAvgSeconds: report.input_buffer_avg_seconds,
    inputBufferMaxSeconds: report.input_buffer_max_seconds,
    inputBufferMinSeconds: report.input_buffer_min_seconds,
  };
}

function fromApiBitrateTrackStats(report: {
  bitrate_1_second: number;
  bitrate_1_minute: number;
}): RtmpInputTrackStatsReport & Mp4InputTrackStatsReport {
  return {
    bitrate1Second: report.bitrate_1_second,
    bitrate1Minute: report.bitrate_1_minute,
  };
}
