# `@swmansion/smelter-node`

Provides API to create and manage Smelter instances for Node.js environment.

When you call `registerOutput` on the Smelter instance, you can pass a `ReactElement` that represents a component tree built from components included in `@swmansion/smelter` package. Those components will define what will be rendered on the output stream.

## Usage

```tsx
import Smelter from '@swmansion/smelter-node';
import { View, Text } from '@swmansion/smelter';

function ExampleApp() {
  return (
    <View>
      <Text style={{ fontSize: 20 }}>Hello world</Text>
    </View>
  );
}

async function run() {
  const smelter = new Smelter();
  await smelter.init();

  // register input/outputs/images/shaders/...

  await smelter.registerOutput('example_output', <ExampleApp />, {
    type: 'rtp_stream',
    port: 8001,
    ip: '127.0.0.1',
    transportProtocol: 'udp',
    video: {
      encoder: { type: 'ffmpeg_h264', preset: 'ultrafast' },
      resolution: { width: 1920, height: 1080 },
    },
    audio: {
      encoder: {
        type: 'opus',
        channels: 'stereo',
      },
    },
  });

  await smelter.start();
}
run();
```

See our [docs](https://smelter.dev/docs) to learn more.

## License

`@swmansion/smelter-node` is MIT licensed, but internally it is downloading and using Smelter server that is licensed
under a [custom license](https://github.com/software-mansion/smelter/blob/master/LICENSE).

## Smelter is created by Software Mansion

[![swm](https://logo.swmansion.com/logo?color=white&variant=desktop&width=150&tag=smelter-github 'Software Mansion')](https://swmansion.com)

Since 2012 [Software Mansion](https://swmansion.com) is a software agency with experience in building web and mobile apps as well as complex multimedia solutions. We are Core React Native Contributors and experts in live streaming and broadcasting technologies. We can help you build your next dream product – [Hire us](https://swmansion.com/contact/projects?utm_source=smelter&utm_medium=readme).
