import { SmelterInstance } from './smelter';
import { routes } from './server/routes';
import { TwitchChannelSuggestions } from './twitch/TwitchChannelMonitor';
import { KickChannelSuggestions } from './kick/KickChannelMonitor';

async function run() {
  console.log('Start monitoring Twitch categories.');
  void TwitchChannelSuggestions.monitor();
  void KickChannelSuggestions.monitor();
  console.log('Start Smelter instance');
  await SmelterInstance.init();

  const port = Number(process.env.SMELTER_DEMO_API_PORT) || 3001;
  console.log(`Start listening on port ${port}`);
  await routes.listen({ port, host: '0.0.0.0' });
}

void run();
