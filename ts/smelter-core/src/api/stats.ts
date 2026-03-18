import type { Api, StatsReport, InputStatsReport, OutputStatsReport } from '@swmansion/smelter';
import {
  fromApiRtpInputStats,
  fromApiWhipInputStats,
  fromApiWhepInputStats,
  fromApiHlsInputStats,
  fromApiRtmpInputStats,
  fromApiMp4InputStats,
} from './stats/inputs.js';
import {
  fromApiWhepOutputStats,
  fromApiWhipOutputStats,
  fromApiHlsOutputStats,
  fromApiMp4OutputStats,
  fromApiRtmpOutputStats,
  fromApiRtpOutputStats,
} from './stats/outputs.js';

export function fromApiStatsReport(report: Api.StatsReport): StatsReport {
  return {
    inputs: Object.fromEntries(
      Object.entries(report.inputs).map(([id, input]) => [id, fromApiInputStatsReport(input)])
    ),
    outputs: Object.fromEntries(
      Object.entries(report.outputs).map(([id, output]) => [id, fromApiOutputStatsReport(output)])
    ),
  };
}

function fromApiInputStatsReport(report: Api.InputStatsReport): InputStatsReport {
  if ('rtp' in report) {
    return { rtp: fromApiRtpInputStats(report.rtp) };
  } else if ('whip' in report) {
    return { whip: fromApiWhipInputStats(report.whip) };
  } else if ('whep' in report) {
    return { whep: fromApiWhepInputStats(report.whep) };
  } else if ('hls' in report) {
    return { hls: fromApiHlsInputStats(report.hls) };
  } else if ('rtmp' in report) {
    return { rtmp: fromApiRtmpInputStats(report.rtmp) };
  } else if ('mp4' in report) {
    return { mp4: fromApiMp4InputStats(report.mp4) };
  }
  throw new Error(`Unknown input stats report type: ${JSON.stringify(report)}`);
}

function fromApiOutputStatsReport(report: Api.OutputStatsReport): OutputStatsReport {
  if ('whep' in report) {
    return { whep: fromApiWhepOutputStats(report.whep) };
  } else if ('whip' in report) {
    return { whip: fromApiWhipOutputStats(report.whip) };
  } else if ('hls' in report) {
    return { hls: fromApiHlsOutputStats(report.hls) };
  } else if ('mp4' in report) {
    return { mp4: fromApiMp4OutputStats(report.mp4) };
  } else if ('rtmp' in report) {
    return { rtmp: fromApiRtmpOutputStats(report.rtmp) };
  } else if ('rtp' in report) {
    return { rtp: fromApiRtpOutputStats(report.rtp) };
  }
  throw new Error(`Unknown output stats report type: ${JSON.stringify(report)}`);
}
