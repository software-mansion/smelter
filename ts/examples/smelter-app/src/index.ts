import { initializeSmelterInstance } from './smelter';
import { app } from './routes';
import { initialCleanup, monitorStreams } from './StreamManager';

async function run() {
  await initialCleanup();
  await monitorStreams();
  await initializeSmelterInstance();

  app.listen(3001);
}

void run();
