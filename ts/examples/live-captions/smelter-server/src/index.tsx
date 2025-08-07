import Smelter from "@swmansion/smelter-node";
import { View } from "@swmansion/smelter";
import { parseMedia } from "@remotion/media-parser";
import { nodeReader } from "@remotion/media-parser/node";
import { Readable, Writable } from "node:stream";
import { encodeADTSHeader } from "./lib/audio.js";
import { spawn } from "node:child_process";
import { resolve } from "node:path";

function App() {
  return <View></View>;
}

class WsWriter extends Writable {
  chunks = Buffer.from([]);

  constructor(private readonly ws: WebSocket) {
    super();
  }

  _write(
    chunk: any,
    _encoding: BufferEncoding,
    callback: (error?: Error | null) => void,
  ): void {
    console.log(chunk);
    this.chunks = Buffer.concat([this.chunks, chunk]);

    if (this.chunks.byteLength > 10_000) {
      this.ws.send(this.chunks);
      this.chunks = Buffer.from([]);
    }

    console.log(this.chunks.byteLength);
    callback();
  }
}

function transcript(src: string) {
  const ws = new WebSocket("http://127.0.0.1:8005/ws");

  ws.addEventListener("open", async () => {
    const adtsReadable = new Readable({
      read() {},
    });

    // prettier-ignore
    const decoder = spawn("ffmpeg", [
        "-probesize", "32",
        "-f",         "aac",
        "-i",         "pipe:0",
        "-f",         "s16le",
        "-acodec",    "pcm_s16le",
        "-ar",        "16000",
        "-ac",        "1",
        "pipe:1",
      ]);

    const writer = new WsWriter(ws);

    adtsReadable.pipe(decoder.stdin);
    decoder.stdout.pipe(writer);

    await parseMedia({
      src: resolve(src),
      reader: nodeReader,
      onAudioTrack: ({ track }) => {
        if (track.codecEnum !== "aac") {
          console.error(
            `Can't process ${src}. Only aac audio samples are currently supported. Detected codec: ${track.codecEnum}`,
          );
          return null;
        }

        return async ({ data }) => {
          const header = encodeADTSHeader(
            track.codecData!.data,
            data.byteLength,
          );
          const adts = Buffer.concat([header, Buffer.from(data)]);
          adtsReadable.push(adts);

          return () => void adtsReadable.push(null);
        };
      },
    });
  });
}

async function run() {
  const smelter = new Smelter();
  await smelter.init();

  transcript("./assets/video.mp4");

  // await ffplayStartPlayerAsync('127.0.0.0', 8001);

  await smelter.registerOutput("output_1", <App />, {
    type: "rtp_stream",
    port: 8001,
    ip: "127.0.0.1",
    transportProtocol: "udp",
    video: {
      encoder: {
        type: "ffmpeg_h264",
        preset: "ultrafast",
      },
      resolution: {
        width: 1920,
        height: 1080,
      },
    },
  });

  // Connect any additional inputs/images/shader you might need before the start.

  await smelter.start();
}

void run();
