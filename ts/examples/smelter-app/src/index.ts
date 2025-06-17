import { initializeSmelterInstance } from './smelter';
import { app } from './routes';

async function run() {
  await initializeSmelterInstance();

  app.listen(3001);
}

void run();
