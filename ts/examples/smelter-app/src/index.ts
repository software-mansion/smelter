import { initializeSmelterInstance } from './smelter';
import { app } from './routes';
import { manageTwitchChannelInfo } from './manageTwitchChannelInfo';

async function run() {
  console.log('Start monitoring Twitch categories.');
  await manageTwitchChannelInfo();
  console.log('Start Smelter instance');
  await initializeSmelterInstance();

  await app.listen({ port: 3001 });
}

void run();
