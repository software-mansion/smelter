# `@swmansion/smelter`

This package provides a set of React components that can be used to define a composition of a video stream. Available components can only be used with React renderers specific for Smelter. We support Node.js runtime with `@swmansion/smelter-node`, browser with `@swmansion/smelter-web-wasm`, and remote server control from the browser with `@swmansion/smelter-web-client`.

Smelter components should not be mixed with other React or React Native components, but you can still use hooks like `useState`/`useEffect` from React.

## Getting started

To try smelter, generate a new project by running:

```
npx create-smelter-app
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

Check out:
- [@swmansion/smelter-node](https://www.npmjs.com/package/@swmansion/smelter-node)
- [@swmansion/smelter-web-client](https://www.npmjs.com/package/@swmansion/smelter-web-client)
- [@swmansion/smelter-web-wasm](https://www.npmjs.com/package/@swmansion/smelter-web-wasm)

to learn how to use it in different environments.

See our [docs](https://smelter.dev/docs) to learn more.

## License

`@swmansion/smelter` package is MIT licensed, but it is only useful when used with Smelter server that is licensed
under a [custom license](https://github.com/software-mansion/smelter/blob/master/LICENSE).

## Smelter is created by Software Mansion

<a href="https://swmansion.com"><img width="150" height="80" alt="Software Mansion" src="https://github.com/user-attachments/assets/cacd6185-78b0-4e76-8767-016d6389bb2b" /></a>

Since 2012 [Software Mansion](https://swmansion.com) is a software agency with experience in building web and mobile apps as well as complex multimedia solutions. We are Core React Native Contributors and experts in live streaming and broadcasting technologies. We can help you build your next dream product – [Hire us](https://swmansion.com/contact/projects?utm_source=smelter&utm_medium=readme).
