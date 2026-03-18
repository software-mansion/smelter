export type RtpInputStatsReport = {
  videoRtp: RtpJitterBufferStatsReport;
  audioRtp: RtpJitterBufferStatsReport;
};

export type RtpJitterBufferStatsReport = {
  packetsLost: number;
  packetsReceived: number;
  bitrate1Second: number;
  bitrate1Minute: number;
  last10Seconds: RtpJitterBufferSlidingWindowStatsReport;
};

export type RtpJitterBufferSlidingWindowStatsReport = {
  packetsLost: number;
  packetsReceived: number;
  /**
   * Measured when packet leaves jitter buffer. This value represents how much time packet has to reach the queue to be processed.
   */
  effectiveBufferAvgSeconds: number;
  effectiveBufferMaxSeconds: number;
  effectiveBufferMinSeconds: number;
  /**
   * Size of the InputBuffer
   */
  inputBufferAvgSeconds: number;
  inputBufferMaxSeconds: number;
  inputBufferMinSeconds: number;
};

export type WhipInputStatsReport = {
  videoRtp: RtpJitterBufferStatsReport;
  audioRtp: RtpJitterBufferStatsReport;
};

export type WhepInputStatsReport = {
  videoRtp: RtpJitterBufferStatsReport;
  audioRtp: RtpJitterBufferStatsReport;
};

export type HlsInputStatsReport = {
  video: HlsInputTrackStatsReport;
  audio: HlsInputTrackStatsReport;
  corruptedPacketsReceived: number;
  corruptedPacketsReceivedLast10Seconds: number;
};

export type HlsInputTrackStatsReport = {
  packetsReceived: number;
  discontinuitiesDetected: number;
  bitrate1Second: number;
  bitrate1Minute: number;
  last10Seconds: HlsInputTrackSlidingWindowStatsReport;
};

export type HlsInputTrackSlidingWindowStatsReport = {
  packetsReceived: number;
  discontinuitiesDetected: number;
  /**
   * Measured when packet leaves jitter buffer. This value represents how much time packet has to reach the queue to be processed.
   */
  effectiveBufferAvgSeconds: number;
  effectiveBufferMaxSeconds: number;
  effectiveBufferMinSeconds: number;
  /**
   * Size of the InputBuffer
   */
  inputBufferAvgSeconds: number;
  inputBufferMaxSeconds: number;
  inputBufferMinSeconds: number;
};

export type RtmpInputStatsReport = {
  video: RtmpInputTrackStatsReport;
  audio: RtmpInputTrackStatsReport;
};
export type RtmpInputTrackStatsReport = {
  bitrate1Second: number;
  bitrate1Minute: number;
};

export type Mp4InputStatsReport = {
  video: Mp4InputTrackStatsReport;
  audio: Mp4InputTrackStatsReport;
};

export type Mp4InputTrackStatsReport = {
  bitrate1Second: number;
  bitrate1Minute: number;
};
