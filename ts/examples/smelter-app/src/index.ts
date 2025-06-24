import { initializeSmelterInstance } from './smelter';
import { app } from './routes';
import { initialCleanup, manageHlsToHlsStreams } from './manageHlsToHlsStreams';
import { manageTwitchChannelInfo } from './manageTwitchChannelInfo';

async function run() {
  await manageTwitchChannelInfo();
  await initialCleanup();
  await manageHlsToHlsStreams();
  await initializeSmelterInstance();

  app.listen(3001);
}

void run();
