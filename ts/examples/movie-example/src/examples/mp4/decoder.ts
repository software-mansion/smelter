import type { MP4ArrayBuffer, MP4File, MP4Info, Sample, TrakBox } from 'mp4box';
import MP4Box, { DataStream } from 'mp4box';

const MAX_FRAMEBUFFER_SIZE = 3;

export class MP4Decoder {
  private file: MP4File;
  private chunks: EncodedVideoChunk[] = [];
  private frames: VideoFrame[] = [];
  private decoder: VideoDecoder;

  public constructor() {
    this.file = MP4Box.createFile();
    this.decoder = new VideoDecoder({
      output: frame => {
        this.frames.push(frame);
      },
      error: error => {
        console.error(`VideoDecoder Error: ${error}`);
      },
    });

    this.file.onReady = info => this.onReady(info);
    this.file.onSamples = (id, user, info) => this.onSamples(id, user, info);
    this.file.onError = (error: string) => {
      console.error(`MP4 Parser Error: ${error}`);
    };
  }

  public decode(videoData: MP4ArrayBuffer) {
    videoData.fileStart = 0;
    this.file.appendBuffer(videoData);
    this.file.flush();
  }

  public nextFrame(): VideoFrame | undefined {
    this.enqueueNextChunks();

    return this.frames.shift();
  }

  private enqueueNextChunks() {
    while (
      this.decoder.decodeQueueSize < MAX_FRAMEBUFFER_SIZE &&
      this.frames.length < MAX_FRAMEBUFFER_SIZE
    ) {
      const chunk = this.chunks.shift();
      if (!chunk) {
        return null;
      }

      this.decoder.decode(chunk);
    }
  }

  private onReady(info: MP4Info) {
    const videoTrack = info.videoTracks[0];
    console.log(`Using codec: ${videoTrack.codec}`);

    const trak = this.file.getTrackById(videoTrack.id);
    if (!trak) {
      return;
    }
    const description = getCodecDescription(trak);
    if (!description) {
      console.error('Codec description not found');
      return;
    }

    this.decoder.configure({
      codec: videoTrack.codec,
      codedWidth: videoTrack.video.width,
      codedHeight: videoTrack.video.height,
      description: description,
    });

    this.file.setExtractionOptions(videoTrack.id);
    this.file.start();
  }

  private onSamples(_id: number, _user: object, samples: Sample[]) {
    for (const sample of samples) {
      const chunk = new EncodedVideoChunk({
        type: sample.is_sync ? 'key' : 'delta',
        timestamp: (sample.cts * 1_000_000) / sample.timescale,
        duration: (sample.duration * 1_000_000) / sample.timescale,
        data: sample.data,
      });

      this.chunks.push(chunk);
    }
  }
}

function getCodecDescription(trak: TrakBox) {
  for (const entry of trak.mdia.minf.stbl.stsd.entries) {
    const box = entry.avcC || entry.hvcC || entry.vpcC || entry.av1C;
    if (box) {
      const stream = new DataStream(undefined, 0, DataStream.BIG_ENDIAN);
      box.write(stream);
      return new Uint8Array(stream.buffer, 8);
    }
  }
}
