import { Mp4, Rescaler, View } from '@swmansion/smelter';
import CommercialMp4 from '../../assets/appjs.mp4';

function Commercial() {
  return (
    <View style={{ backgroundColor: '#161127' }}>
      <Rescaler
        style={{
          borderRadius: 24,
          borderColor: 'white',
          borderWidth: 1,
        }}>
        <Mp4 source={new URL(CommercialMp4, import.meta.url).toString()} />
      </Rescaler>
    </View>
  );
}
export default Commercial;
