import { View, InputStream, Rescaler } from '@swmansion/smelter';

const WIDTH = 1920;
const HEIGHT = 1080;

// PiP streams are scaled to 10% of the output width; height preserves aspect ratio (16:9 assumed)
const PIP_WIDTH = Math.round(WIDTH * 0.2);
const PIP_HEIGHT = Math.round(HEIGHT * 0.2);

const MARGIN = 20;

type PipProps = {
  mainInputId: string;
  topLeftInputId: string;
  topRightInputId: string;
  bottomLeftInputId: string;
  bottomRightInputId: string;
};

export default function Pip({
  mainInputId,
  topLeftInputId,
  topRightInputId,
  bottomLeftInputId,
  bottomRightInputId,
}: PipProps) {
  return (
    <View style={{ width: WIDTH, height: HEIGHT }}>
      {/* Main stream — fills entire output */}
      <View style={{ width: WIDTH, height: HEIGHT, top: 0, left: 0 }}>
        <Rescaler style={{ width: WIDTH, height: HEIGHT }}>
          <InputStream inputId={mainInputId} />
        </Rescaler>
      </View>

      {/* Top-left PiP */}
      <Rescaler style={{ width: PIP_WIDTH, height: PIP_HEIGHT, top: MARGIN, left: MARGIN }}>
        <InputStream inputId={topLeftInputId} />
      </Rescaler>

      {/* Top-right PiP */}
      <Rescaler style={{ width: PIP_WIDTH, height: PIP_HEIGHT, top: MARGIN, right: MARGIN }}>
        <InputStream inputId={topRightInputId} />
      </Rescaler>

      {/* Bottom-left PiP */}
      <Rescaler style={{ width: PIP_WIDTH, height: PIP_HEIGHT, bottom: MARGIN, left: MARGIN }}>
        <InputStream inputId={bottomLeftInputId} />
      </Rescaler>

      {/* Bottom-right PiP */}
      <Rescaler style={{ width: PIP_WIDTH, height: PIP_HEIGHT, bottom: MARGIN, right: MARGIN }}>
        <InputStream inputId={bottomRightInputId} />
      </Rescaler>
    </View>
  );
}
