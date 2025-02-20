import Smelter from '@swmansion/smelter-node';
import { View, Text, Image } from '@swmansion/smelter';
import { useEffect, useState } from 'react';
import { ffplayStartPlayerAsync, sleep } from './utils';

type PartialTextProps = {
  text: string;
  transitionMs: number;
};

function PartialText(props: PartialTextProps) {
  const intervalMs = props.transitionMs / props.text.length;

  const [textPart, updateTextPart] = useState({
    characters: props.text.length,
    shrink: false,
  });

  useEffect(() => {
    const timeout = setTimeout(() => {
      if (textPart.characters === 1 && textPart.shrink) {
        updateTextPart({ characters: 1, shrink: false });
      } else if (textPart.characters === props.text.length && !textPart.shrink) {
        updateTextPart({ characters: props.text.length, shrink: true });
      } else {
        updateTextPart({
          characters: textPart.shrink ? textPart.characters - 1 : textPart.characters + 1,
          shrink: textPart.shrink,
        });
      }
    }, intervalMs);
    return () => {
      clearTimeout(timeout);
    };
  }, [textPart]);

  return (
    <View>
      <Text style={{ fontSize: 40, fontFamily: 'Noto Sans' }}>
        {props.text.substring(0, textPart.characters)}
      </Text>
    </View>
  );
}

function ExampleApp() {
  return (
    <View style={{ direction: 'column' }}>
      <PartialText text="Example partial text that transition in 1 second" transitionMs={1_000} />
      <PartialText text="Example partial text that transition in 2 second" transitionMs={2_000} />
      <PartialText text="Example partial text that transition in 5 second" transitionMs={5_000} />
      <Image imageId="image_1" />
    </View>
  );
}

async function run() {
  const smelter = new Smelter();
  await smelter.init();

  await ffplayStartPlayerAsync('127.0.0.1', 8001);
  await sleep(2000);

  await smelter.registerFont(
    'https://fonts.gstatic.com/s/notosans/v36/o-0mIpQlx3QUlC5A4PNB6Ryti20_6n1iPHjcz6L1SoM-jCpoiyD9A-9a6Vc.ttf'
  );
  await smelter.registerImage('image_1', {
    assetType: 'svg',
    url: 'https://smelter.dev/img/logo.svg',
    resolution: { width: 300, height: 300 },
  });

  await smelter.registerOutput('output_1', <ExampleApp />, {
    type: 'rtp_stream',
    port: 8001,
    ip: '127.0.0.1',
    transportProtocol: 'udp',
    video: {
      encoder: {
        type: 'ffmpeg_h264',
        preset: 'ultrafast',
      },
      resolution: {
        width: 1920,
        height: 1080,
      },
    },
  });

  await smelter.start();
}

void run();
