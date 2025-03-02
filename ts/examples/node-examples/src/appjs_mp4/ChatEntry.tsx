import { View } from '@swmansion/smelter';

const CHAR_SIZE = 72;

export type Message = {
  id: number;
  text: string;
};

function ChatEntry(props: { msg: Message; width: number }) {
  return (
    <View style={{ direction: 'column', paddingBottom: 28, height: CHAR_SIZE + 28 }}>
      <View key={props.msg.id}>
        <View
          style={{
            width: CHAR_SIZE * 1.3,
            height: CHAR_SIZE,
            backgroundColor: '#FFFFFF22',
            borderRadius: 24,
          }}
        />
        <View style={{ width: CHAR_SIZE * 0.3, height: CHAR_SIZE }} />
        <View
          style={{
            width: props.msg.text.length * 10,
            height: CHAR_SIZE,
            backgroundColor: '#FFFFFF22',
            borderRadius: 24,
          }}
        />
      </View>
    </View>
  );
}

export default ChatEntry;
