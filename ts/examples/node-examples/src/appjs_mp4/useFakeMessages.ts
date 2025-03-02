import { useEffect, useState } from 'react';
import type { Message } from './ChatEntry';
import { useAfterTimestamp } from '@swmansion/smelter';

const start: [number, Message][] = [
  [6800, { id: 17, text: 'Hey team, quick check: are we good for today’s sprint?' }],
  [6900, { id: 18, text: 'Yep, just finishing up.' }],
  [7000, { id: 19, text: 'Checking last commits now. Any concerns?' }],
  [7100, { id: 20, text: 'Looks alright, need any assistance?' }],
  [7200, { id: 21, text: 'I’m managing fine, thanks though!' }],
  [7300, { id: 22, text: 'Okay. What’s the status on backend issues?' }],
  [7400, { id: 23, text: 'Fixed the bug, just one final test left.' }],
  [7500, { id: 24, text: 'Good on this end, wrapping up the integration now.' }],
  [7600, { id: 25, text: 'Setting up the final build environment.' }],
  [7700, { id: 26, text: 'Almost done, just a few more minutes.' }],
  [7800, { id: 27, text: 'Monitoring the build, everything looking okay so far.' }],
  [7900, { id: 28, text: 'Any final checks or issues needing attention?' }],
  [8000, { id: 29, text: 'Finished the updates, ready for review.' }],
  [8400, { id: 30, text: 'Going through the changes now.' }],
  [9000, { id: 31, text: 'All looks set from this side.' }],
  [9500, { id: 32, text: 'Noticed any issues with the deployment yet?' }],
  [10000, { id: 33, text: 'Some slight server timeouts earlier, but all clear now.' }],
  [10250, { id: 34, text: 'Stability confirmed. Proceeding with the next steps.' }],
  [10800, { id: 35, text: 'Excellent, starting to prepare for the demo with the client.' }],
  [11400, { id: 36, text: 'Has anyone checked if the latest data is ready?' }],
  [12000, { id: 37, text: `Data's integrated and updated for the demo.` }],
];

export function useFakeMessages(): Message[] {
  const [messages, setMessages] = useState<Message[]>([]);
  const [nextTimestamp, setNextTimestamp] = useState(0);
  const after = useAfterTimestamp(nextTimestamp);

  useEffect(() => {
    if (!after) {
      return;
    }
    const result = start.find(([timeStamp, _msg]) => timeStamp > nextTimestamp);
    const timestamp = result ? result[0] : Number.POSITIVE_INFINITY;

    setMessages(start.filter(([ts, _msg]) => ts < timestamp).map(([_ts, msg]) => msg));
    setNextTimestamp(timestamp);
  }, [after, nextTimestamp]);

  return messages;
}
