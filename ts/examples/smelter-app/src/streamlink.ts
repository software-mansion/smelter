import { spawn } from './utils';

export async function hlsUrlForTwitchChannel(channelId: string) {
  const url = `https://www.twitch.tv/${channelId}`;
  return await getHlsPlaylistUrl(url);
}

export async function hlsUrlForKickChannel(channelId: string) {
  const url = `https://kick.com/${channelId}`;
  return await getHlsPlaylistUrl(url);
}

async function getHlsPlaylistUrl(url: string): Promise<string> {
  const streamlinkOutput = await spawn('streamlink', ['--stream-url', url, '720p,720p60,best'], {
    stdio: 'pipe',
  });
  return streamlinkOutput.stdout.trim();
}
