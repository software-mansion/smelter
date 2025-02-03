# `@swmansion/smelter`

This package provides a set of React components that can be used to define a composition of a video stream. Available components can only be used with React renderers specific for Smelter. Currently, we only support Node.js runtime with `@swmansion/smelter-node` package, but support for more runtimes is planned in the future.

Smelter components should not be mixed with other React or React Native components, but you can still use hooks like `useState`/`useEffect` from React.

## Getting started

To try smelter, generate a new project by running:

```
npm create smelter-app
```

## Usage

```tsx
import { View, Text, InputStream, Rescaler } from '@swmansion/smelter';

function ExampleApp() {
  return (
    <View style={{ direction: 'column' }}>
      <Rescaler style={{ rescaleMode: 'fill' }}>
        <InputStream inputId="example_input_1" />
      </Rescaler>
      <Text style={{ fontSize: 20 }}>Example label</Text>
    </View>
  );
}
```

Check out [@swmansion/smelter-node](https://www.npmjs.com/package/@swmansion/smelter-node) to learn how to use those components for video composition.

See our [docs](https://smelter.dev/docs) to learn more.

## License

`@swmansion/smelter` package is MIT licensed, but it is only useful when used with Smelter server that is licensed
under a [custom license](https://github.com/software-mansion/smelter/blob/master/LICENSE).

## Smelter is created by Software Mansion

[![swm](https://logo.swmansion.com/logo?color=white&variant=desktop&width=150&tag=smelter-github 'Software Mansion')](https://swmansion.com)

Since 2012 [Software Mansion](https://swmansion.com) is a software agency with experience in building web and mobile apps as well as complex multimedia solutions. We are Core React Native Contributors and experts in live streaming and broadcasting technologies. We can help you build your next dream product – [Hire us](https://swmansion.com/contact/projects?utm_source=smelter&utm_medium=readme).
