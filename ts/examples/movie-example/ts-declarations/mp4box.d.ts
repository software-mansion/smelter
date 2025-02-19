declare module 'mp4box' {
  export class DataStream {
    constructor(buffer?: ArrayBuffer, byteOffset?: number, endianness?: boolean);

    get buffer(): ArrayBuffer;
    set buffer(v: ArrayBuffer);

    static LITTLE_ENDIAN: boolean;
    static BIG_ENDIAN: boolean;
  }

  export interface MP4File {
    onReady?: (info: MP4Info) => void;
    onError?: (e: string) => void;
    onSamples?: (id: number, user: object, samples: Sample[]) => void;

    getTrackById(id: number): TrakBox | undefined;

    appendBuffer(data: MP4ArrayBuffer): number;
    start(): void;
    stop(): void;
    flush(): void;

    setExtractionOptions(id: number, user?: object, options?: ExtractionOptions): void;
  }

  export interface MP4MediaTrack {
    id: number;
    movie_duration: number;
    track_width: number;
    track_height: number;
    timescale: number;
    duration: number;
    bitrate: number;
    codec: string;
  }

  export interface MP4VideoTrack extends MP4MediaTrack {
    video: {
      width: number;
      height: number;
    };
  }

  export interface MP4AudioTrack extends MP4MediaTrack {
    audio: {
      sample_size: number;
      sample_rate: number;
      channel_count: number;
    };
  }

  export type MP4Track = MP4VideoTrack | MP4AudioTrack;

  export interface MP4Info {
    duration: number;
    timescale: number;
    tracks: MP4Track[];
    audioTracks: MP4AudioTrack[];
    videoTracks: MP4VideoTrack[];
  }

  export interface Sample {
    timescale: number;
    data: ArrayBuffer;
    size: number;
    duration: number;
    cts: number;
    dts: number;
    is_sync: boolean;
    depends: number;
  }

  export interface ExtractionOptions {
    nbSamples: number;
  }

  export type MP4ArrayBuffer = ArrayBuffer & { fileStart: number };

  export class Box {
    write(stream: DataStream): void;
  }

  export class TrakBox extends Box {
    mdia: MdiaBox;
  }

  export class MdiaBox extends Box {
    minf: MinfBox;
  }

  export class MinfBox extends Box {
    stbl: StblBox;
  }

  export class StblBox extends Box {
    stsd: StsdBox;
  }

  export class StsdBox extends Box {
    entries: Description[];
  }

  export interface Description {
    avcC?: Box;
    hvcC?: Box;
    vpcC?: Box;
    av1C?: Box;
  }

  export function createFile(): MP4File;
}
