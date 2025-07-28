import { initializeSmelterInstance } from './smelter';
import { app } from './routes';
import { initialCleanup, manageHlsToHlsStreams } from './manageHlsToHlsStreams';
import { manageTwitchChannelInfo } from './manageTwitchChannelInfo';

async function run() {
  console.log('Stop old FFmpeg processes and remove files.');
  await initialCleanup();
  console.log('Start monitoring Twitch categories.');
  await manageTwitchChannelInfo();
  console.log('Run HLS-to-HLS pipeline for each available stream.');
  await manageHlsToHlsStreams();
  console.log('Start Smelter instance');
  await initializeSmelterInstance();

  app.listen(3001);
}

void run();
