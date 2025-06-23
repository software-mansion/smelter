import { initializeSmelterInstance } from './smelter';
import { app } from './routes';
import * as fs from 'fs-extra';
import { SMELTER_WORKDIR } from './addTwitchStream';

async function run() {
  await Promise.allSettled([fs.remove(SMELTER_WORKDIR)]);
  await fs.mkdirp(SMELTER_WORKDIR);
  await initializeSmelterInstance();

  app.listen(3001);
}

void run();
