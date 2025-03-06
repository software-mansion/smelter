import type {
  MP4ArrayBuffer,
  MP4AudioTrack,
  MP4File,
  MP4Info,
  MP4VideoTrack,
  Sample,
} from 'mp4box';
import MP4Box, { DataStream } from 'mp4box';
import type {
  ContainerInfo,
  EncodedAudioPayload,
  EncodedVideoPayload,
  EncodedSource,
} from '../input';
import type { Logger } from 'pino';
import { Queue } from '@datastructures-js/queue';
import type { Framerate } from '../../../compositor/compositor';
import { assert } from '../../../utils';

const AudioTrack = { type: 'audio-track' };
const VideoTrack = { type: 'video-track' };

export type Mp4Metadata = {
  video?: {
    decoderConfig: VideoDecoderConfig;
    framerate: Framerate;
    trackId: number;
    frameCount: number;
    durationMs: number;
  };
  audio?: {
    decoderConfig: AudioDecoderConfig;
    trackId: number;
    durationMs: number;
    sampleCount: number;
  };
};

export class Mp4Demuxer implements EncodedSource {
  private file: MP4File;
  private logger: Logger;
  private ptsOffset?: number;

  private videoChunks: Queue<EncodedVideoChunk>;
  private videoTrackFinished: boolean = false;

  private audioChunks: Queue<EncodedAudioChunk>;
  // @ts-ignore
  private audioTrackFinished: boolean = false;

  private firstVideoChunkPromise: Promise<void>;
  private firstAudioChunkPromise: Promise<void>;

  private readyPromise: Promise<Mp4Metadata>;
  private mp4Metadata?: Mp4Metadata;

  public constructor(data: ArrayBuffer, logger: Logger) {
    this.logger = logger;
    this.videoChunks = new Queue();
    this.audioChunks = new Queue();

    this.file = MP4Box.createFile();
    this.readyPromise = new Promise<Mp4Metadata>((res, rej) => {
      this.file.onReady = info => {
        try {
          res(this.parseMp4Info(info));
        } catch (err: any) {
          rej(err);
        }
      };
      this.file.onError = (error: string) => {
        this.logger.error(`MP4Demuxer error: ${error}`);
        rej(new Error(error));
      };
    });

    let firstVideoChunkCb: (() => void) | undefined;
    this.firstVideoChunkPromise = new Promise<void>((res, _rej) => {
      firstVideoChunkCb = res;
    });

    let firstAudioChunkCb: (() => void) | undefined;
    this.firstAudioChunkPromise = new Promise<void>((res, _rej) => {
      firstAudioChunkCb = res;
    });

    this.file.onSamples = (id, user, samples) => {
      if (user === AudioTrack) {
        this.onAudioSamples(samples);
        if (id === this.mp4Metadata?.audio?.trackId) {
          firstAudioChunkCb?.();
        }
      }
      if (user == VideoTrack) {
        this.onVideoSamples(samples);
        if (id === this.mp4Metadata?.video?.trackId) {
          firstVideoChunkCb?.();
        }
      }
    };

    const mp4Data = data as MP4ArrayBuffer;
    mp4Data.fileStart = 0;

    this.file.appendBuffer(mp4Data);
  }

  public async init(): Promise<void> {
    this.mp4Metadata = await this.readyPromise;
    if (this.mp4Metadata.video) {
      this.file.setExtractionOptions(this.mp4Metadata.video.trackId, VideoTrack);
    }
    if (this.mp4Metadata.audio) {
      this.file.setExtractionOptions(this.mp4Metadata.audio.trackId, AudioTrack);
    }
    this.file.start();

    // by flushing we are signaling that there won't be any new
    // chunks added
    this.file.flush();

    if (this.mp4Metadata.video) {
      await this.firstVideoChunkPromise;
    }
    if (this.mp4Metadata.audio) {
      await this.firstAudioChunkPromise;
    }
  }

  public getMetadata(): ContainerInfo {
    assert(this.mp4Metadata, 'Mp4 metadata not available, call `init` first.');
    return {
      video: this.mp4Metadata.video && {
        durationMs: this.mp4Metadata.video.durationMs,
        decoderConfig: this.mp4Metadata.video.decoderConfig,
      },
      audio: this.mp4Metadata.audio && {
        durationMs: this.mp4Metadata.audio.durationMs,
        decoderConfig: this.mp4Metadata.audio.decoderConfig,
      },
    };
  }

  public nextAudioChunk(): EncodedAudioPayload | undefined {
    const chunk = this.audioChunks.pop();
    if (chunk) {
      return { type: 'chunk', chunk };
    } else if (this.videoTrackFinished) {
      return { type: 'eos' };
    }
    return;
  }

  public nextVideoChunk(): EncodedVideoPayload | undefined {
    const chunk = this.videoChunks.pop();
    if (chunk) {
      return { type: 'chunk', chunk };
    } else if (this.videoTrackFinished) {
      return { type: 'eos' };
    }
    return;
  }

  public close(): void {
    this.file.stop();
  }

  private parseMp4Info(info: MP4Info): Mp4Metadata {
    return {
      video: info.videoTracks[0] && parseMp4VideoInfo(this.file, info.videoTracks[0]),
      audio: info.audioTracks[0] && parseMp4AudioInfo(this.file, info.audioTracks[0]),
    };
  }

  private onVideoSamples(samples: Sample[]) {
    assert(this.mp4Metadata?.video);

    for (const sample of samples) {
      const pts = (sample.cts * 1_000_000) / sample.timescale;
      if (this.ptsOffset === undefined) {
        this.ptsOffset = -pts;
      }

      const chunk = new EncodedVideoChunk({
        type: sample.is_sync ? 'key' : 'delta',
        timestamp: pts + this.ptsOffset,
        duration: (sample.duration * 1_000_000) / sample.timescale,
        data: sample.data,
      });

      this.videoChunks.push(chunk);

      if (sample.number === this.mp4Metadata.video.frameCount - 1) {
        this.videoTrackFinished = true;
      }
    }
  }

  private onAudioSamples(samples: Sample[]) {
    assert(this.mp4Metadata?.audio);

    for (const sample of samples) {
      const pts = (sample.cts * 1_000_000) / sample.timescale;
      if (this.ptsOffset === undefined) {
        this.ptsOffset = -pts;
      }

      const chunk = new EncodedAudioChunk({
        type: sample.is_sync ? 'key' : 'delta',
        timestamp: pts + this.ptsOffset,
        duration: (sample.duration * 1_000_000) / sample.timescale,
        data: sample.data,
      });

      this.audioChunks.push(chunk);

      // TODO: check that
      if (sample.number === (this.mp4Metadata.audio?.sampleCount ?? 0) - 1) {
        this.audioTrackFinished = true;
      }
    }
  }
}

function parseMp4AudioInfo(file: MP4File, track: MP4AudioTrack): Mp4Metadata['audio'] {
  const durationMs = (track.movie_duration / track.movie_timescale) * 1000;
  const codecDescription = getAudioCodecDescription(file, track.id);

  const decoderConfig: AudioDecoderConfig = {
    codec: track.codec,
    description: codecDescription,
    numberOfChannels: track.audio.channel_count,
    sampleRate: track.audio.sample_rate,
  };

  return {
    trackId: track.id,
    durationMs,
    decoderConfig,
    sampleCount: track.nb_samples,
  };
}

function parseMp4VideoInfo(file: MP4File, track: MP4VideoTrack): Mp4Metadata['video'] {
  const durationMs = (track.movie_duration / track.movie_timescale) * 1000;
  const codecDescription = getCodecDescription(file, track.id);
  const frameCount = track.nb_samples;

  const decoderConfig = {
    codec: track.codec,
    codedWidth: track.video.width,
    codedHeight: track.video.height,
    description: codecDescription,
  };
  const framerate = {
    num: track.timescale,
    den: 1000,
  };

  return {
    trackId: track.id,
    durationMs,
    frameCount,
    decoderConfig,
    framerate,
  };
}

function getCodecDescription(file: MP4File, trackId: number): Uint8Array {
  const track = file.getTrackById(trackId);
  if (!track) {
    throw new Error('Track does not exist');
  }

  for (const entry of track.mdia.minf.stbl.stsd.entries) {
    const box = entry.avcC || entry.hvcC || entry.vpcC || entry.av1C;
    if (box) {
      const stream = new DataStream(undefined, 0, DataStream.BIG_ENDIAN);
      box.write(stream);
      return new Uint8Array(stream.buffer, 8);
    }
  }

  throw new Error('Codec description not found');
}

function getAudioCodecDescription(file: MP4File, trackId: number): Uint8Array {
  const track = file.getTrackById(trackId);
  if (!track) {
    throw new Error('Track does not exist');
  }

  for (const entry of track.mdia.minf.stbl.stsd.entries) {
    return (entry as any).esds.esd.descs[0].descs[0].data;
  }

  throw new Error('Codec description not found');
}
