import { View, Mp4, Rescaler } from '@swmansion/smelter';
import path from 'path';

export function DayTwoScene() {
  return (
    <View>
      <Rescaler>
        <Mp4 source={path.join(__dirname, 'assets/game.mp4')} />
      </Rescaler>
    </View>
  );
}
