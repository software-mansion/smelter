import { useEffect } from 'react';
import { fakeMessages, getRandomTwitchProfile } from '../data';
import { v4 as uuid } from 'uuid';

export type TMessage = {
  id: string;
  user: string;
  color: string;
  text: string;
};

export type ChatStubOpts = {
  onMessage: (message: TMessage) => void;
};

/** Generates random message every 3 seconds */
export function useChatStub({ onMessage }: ChatStubOpts) {
  useEffect(() => {
    let timeout: NodeJS.Timeout;

    const sendMessage = () => {
      const text = fakeMessages[Math.round(Math.random() * (fakeMessages.length - 1))];
      const { user, color } = getRandomTwitchProfile();

      onMessage({
        id: uuid(),
        user,
        color,
        text,
      });

      timeout = setTimeout(sendMessage, Math.random() * 4000 + 500);
    };

    timeout = setTimeout(sendMessage, 500);

    return () => clearTimeout(timeout);
  }, []);
}
