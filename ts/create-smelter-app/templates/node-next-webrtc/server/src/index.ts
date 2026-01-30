import { SmelterInstance } from './smelter';
import { routes } from './routes';

async function run() {
  await SmelterInstance.init();

  await routes.listen({ port: 3001, host: '0.0.0.0' });
}

void run();
