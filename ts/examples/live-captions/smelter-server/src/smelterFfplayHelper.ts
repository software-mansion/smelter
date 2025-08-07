import os from 'os';
import path from 'path';
import fs from 'fs';
import type { ChildProcess, SpawnOptions } from 'child_process';
import { spawn as nodeSpawn } from 'child_process';

const TMP_SDP_DIR = path.join(os.tmpdir(), 'smelter-sdp');

/**
 * Util function that displays video sent over RTP to the specified port.
 * This is only for debugging, do not use for production use cases.
 */
export async function ffplayStartPlayerAsync(ip: string, videoPort: number): Promise<void> {
  if (!fs.existsSync(TMP_SDP_DIR)) {
    fs.mkdirSync(TMP_SDP_DIR);
  }
  const sdpFilePath = path.join(TMP_SDP_DIR, `video_input_${videoPort}.sdp`);
  writeVideoSdpFile(ip, videoPort, sdpFilePath);
  void spawn('ffplay', ['-protocol_whitelist', 'file,rtp,udp', sdpFilePath], {});
  await new Promise<void>(res => setTimeout(() => res(), 2000));
}

interface SpawnPromise extends Promise<void> {
  child: ChildProcess;
}

function spawn(command: string, args: string[], options: SpawnOptions): SpawnPromise {
  const child = nodeSpawn(command, args, {
    stdio: 'inherit',
    ...options,
  });
  const promise = new Promise((res, rej) => {
    child.on('exit', code => {
      if (code === 0) {
        res();
      } else {
        rej(new Error(`Command "${command} ${args.join(' ')}" failed with exit code ${code}.`));
      }
    });
  }) as SpawnPromise;
  promise.child = child;
  return promise;
}

function writeVideoSdpFile(ip: string, port: number, destination: string): void {
  fs.writeFileSync(
    destination,
    `
v=0
o=- 0 0 IN IP4 ${ip}
s=No Name
c=IN IP4 ${ip}
m=video ${port} RTP/AVP 96
a=rtpmap:96 H264/90000
a=fmtp:96 packetization-mode=1
a=rtcp-mux
`
  );
}
