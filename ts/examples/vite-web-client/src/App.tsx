import './App.css'
import { useSmelter } from './useSmelter'
import WhepOutputVideo from './WhepOutputVideo'
import { View, Text, Mp4, Rescaler } from '@swmansion/smelter'

function App() {
  const smelter = useSmelter("http://localhost:8081")

  if (!smelter) {
    return <>
      <p>Start smelter on port 8081 to run this example</p>
    </>
  }

  return (
    <>
      <WhepOutputVideo
        smelter={smelter} autoPlay controls muted
        style={{ maxWidth: '100%', maxHeight: '100%' }}
      >
        <SmelterScene />
      </WhepOutputVideo>
    </>
  )
}

function SmelterScene() {
  return (
    <View style={{ direction: 'column' }}>
      <Rescaler>
        <Mp4 source="https://smelter.dev/videos/template-scene-race.mp4" />
      </Rescaler>
      <View style={{ bottom: 0, left: 0, padding: 10, height: 60 }}>
        <Text>Example video composition</Text>
      </View>
    </View>
  )
}

export default App
