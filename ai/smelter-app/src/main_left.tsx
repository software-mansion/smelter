import { View, InputStream, Rescaler } from '@swmansion/smelter';

const WIDTH = 1920;
const HEIGHT = 1080;

const GAP_MAIN = 8;  // gap between main stream and side column
const GAP_SIDE = 4;  // gap between side streams

const MAIN_WIDTH = Math.round(WIDTH * 0.8);
const SIDE_WIDTH = WIDTH - MAIN_WIDTH - GAP_MAIN;
const SIDE_STREAM_HEIGHT = Math.round((HEIGHT - GAP_SIDE * 3) / 4);

type MainLeftLayoutProps = {
  mainInputId: string;
  firstInputId: string;
  secondInputId: string;
  thirdInputId: string;
  fourthInputId: string;
};

export default function MainLeftLayout({
  mainInputId,
  firstInputId,
  secondInputId,
  thirdInputId,
  fourthInputId,
}: MainLeftLayoutProps) {
  return (
    <View style={{ width: WIDTH, height: HEIGHT, direction: 'row' }}>

      {/* Main stream — 80% width, aspect ratio preserved */}
      <Rescaler style={{ width: MAIN_WIDTH, height: HEIGHT }}>
        <InputStream inputId={mainInputId} />
      </Rescaler>

      {/* Gap between main stream and side column */}
      <View style={{ width: GAP_MAIN, height: HEIGHT }} />

      {/* Side column — 4 streams stacked vertically */}
      <View style={{ width: SIDE_WIDTH, height: HEIGHT, direction: 'column' }}>

        <Rescaler style={{ width: SIDE_WIDTH, height: SIDE_STREAM_HEIGHT }}>
          <InputStream inputId={firstInputId} />
        </Rescaler>

        <View style={{ width: SIDE_WIDTH, height: GAP_SIDE }} />

        <Rescaler style={{ width: SIDE_WIDTH, height: SIDE_STREAM_HEIGHT }}>
          <InputStream inputId={secondInputId} />
        </Rescaler>

        <View style={{ width: SIDE_WIDTH, height: GAP_SIDE }} />

        <Rescaler style={{ width: SIDE_WIDTH, height: SIDE_STREAM_HEIGHT }}>
          <InputStream inputId={thirdInputId} />
        </Rescaler>

        <View style={{ width: SIDE_WIDTH, height: GAP_SIDE }} />

        <Rescaler style={{ width: SIDE_WIDTH, height: SIDE_STREAM_HEIGHT }}>
          <InputStream inputId={fourthInputId} />
        </Rescaler>

      </View>
    </View>
  );
}
