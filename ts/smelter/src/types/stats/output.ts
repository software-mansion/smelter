export type WhepOutputStatsReport = {
  video: WhepOutputTrackStatsReport;
  audio: WhepOutputTrackStatsReport;
  connectedPeers: number;
};
export type WhepOutputTrackStatsReport = {
  bitrate1Second: number;
  bitrate1Minute: number;
};
export type WhipOutputStatsReport = {
  video: WhipOutputTrackStatsReport;
  audio: WhipOutputTrackStatsReport;
  isConnected: boolean;
};
export type WhipOutputTrackStatsReport = {
  bitrate1Second: number;
  bitrate1Minute: number;
};
export type HlsOutputStatsReport = {
  video: HlsOutputTrackStatsReport;
  audio: HlsOutputTrackStatsReport;
};
export type HlsOutputTrackStatsReport = {
  bitrate1Second: number;
  bitrate1Minute: number;
};
export type Mp4OutputStatsReport = {
  video: Mp4OutputTrackStatsReport;
  audio: Mp4OutputTrackStatsReport;
};
export type Mp4OutputTrackStatsReport = {
  bitrate1Second: number;
  bitrate1Minute: number;
};
export type RtmpOutputStatsReport = {
  video: RtmpOutputTrackStatsReport;
  audio: RtmpOutputTrackStatsReport;
};
export type RtmpOutputTrackStatsReport = {
  bitrate1Second: number;
  bitrate1Minute: number;
};
export type RtpOutputStatsReport = {
  video: RtpOutputTrackStatsReport;
  audio: RtpOutputTrackStatsReport;
};
export type RtpOutputTrackStatsReport = {
  bitrate1Second: number;
  bitrate1Minute: number;
};
