import { spawn } from 'child_process';

/**
 * Util function that starts RTMP server and display incoming streams with ffplay.
 */
export async function ffplayStartPlayerAsync(port: number): Promise<void> {
  const command = 'bash';
  const args = [
    '-c',
    `ffmpeg -f flv -listen 1 -i rtmp://0.0.0.0:${port} -vcodec copy  -f flv - | ffplay -f flv -i -`,
  ];
  const child = spawn(command, args, { stdio: 'inherit' });
  child.on('exit', code => {
    if (code !== 0) {
      console.error(`Command "${command} ${args.join(' ')}" failed with exit code ${code}.`);
    }
  });
  await new Promise<void>(res => setTimeout(() => res(), 2000));
}
