import { View } from '@swmansion/smelter';
import ChatEntry from './ChatEntry';
import { useFakeMessages } from './useFakeMessages';

type ChatProps = {
  width: number;
  height: number;
};

function Chat(props: ChatProps) {
  const messages = useFakeMessages();

  return (
    <View
      style={{
        borderRadius: 16,
        width: props.width,
        height: props.height,
      }}>
      <View style={{ height: 4500, bottom: 0, left: 0, direction: 'column' }}>
        <View />
        {messages.map(msg => {
          return <ChatEntry key={msg.id} width={props.width} msg={msg} />;
        })}
      </View>
    </View>
  );
}

export default Chat;
