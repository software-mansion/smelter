import { SmelterInstance } from './smelter';
import { routes } from './server/routes';
import { TwitchChannelSuggestions } from './twitch/ChannelMonitor';

async function run() {
  console.log('Start monitoring Twitch categories.');
  void TwitchChannelSuggestions.monitor();
  console.log('Start Smelter instance');
  await SmelterInstance.init();

  console.log('Start listening on port 3001');
  await routes.listen({ port: 3001, host: '0.0.0.0' });
}

void run();
